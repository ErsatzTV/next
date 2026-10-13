use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ersatztv_channel::error::{ChannelError, IoContext};
use ersatztv_core::{HEARTBEAT_FILE_NAME, HEARTBEAT_FILE_TIMEOUT};
use ffpipeline::pipeline::PtsOffset;
use ffpipeline::web_vtt::{Cue, format_vtt_ts};
use time::OffsetDateTime;
use time::macros::format_description;

const MIN_SEGMENTS: usize = 4;

// 12s
const PUBLISH_LEAD: Duration =
    Duration::from_secs(ffpipeline::pipeline::SEGMENT_SECONDS as u64 * 3);

const MPEGTS_WRAP: u64 = 1 << 33;

// LOCAL stays 0 because VLC 3 reads any other LOCAL as 0. MPEGTS stays near the segment PTS
// because hls.js uses the segment PTS to resolve MPEGTS rollover.
const SUBTITLE_ANCHOR_INTERVAL_SECS: u64 = 600;
// hls.js ignores the timestamp map when MPEGTS equals the stream's first PTS, which is 0.
// One tick is much smaller than the ms precision of cue times.
const SUBTITLE_ANCHOR_NUDGE_TICKS: u64 = 1;

// smaller gaps are frame rounding
pub const SHORTFALL_TOLERANCE: Duration =
    Duration::from_secs(ffpipeline::pipeline::SEGMENT_SECONDS as u64);

#[derive(Clone)]
pub struct SubtitleSource {
    pub cues: Arc<Vec<Cue>>,
    pub(crate) cursor: usize,
    pub next_segment_source_offset: Duration,
    in_point: Duration,
    out_point: Duration,
    stream_offset_ms: Option<i64>,
}

impl SubtitleSource {
    pub fn new(cues: Arc<Vec<Cue>>, in_point: Duration, out_point: Duration) -> Self {
        SubtitleSource {
            cues,
            cursor: 0,
            next_segment_source_offset: in_point,
            in_point,
            out_point,
            stream_offset_ms: None,
        }
    }
}

#[derive(Clone)]
pub struct PlaylistManager {
    output_folder: PathBuf,
    ready_file: PathBuf,
    heartbeat_file: PathBuf,
    generated_playlist_file: String,
    generated_subtitle_playlist_file: String,
    ffmpeg_playlist_file: String,
    ready: bool,
    ready_at: Option<Instant>,

    segments: VecDeque<Segment>,
    discontinuity_before: HashSet<String>,
    media_sequence: u64,
    last_served_media_sequence: u64,
    discontinuity_sequence: u64,
    target_duration: u32,
    target_duration_f64: f64,
    pending_discontinuity: bool,
    last_segment_end: OffsetDateTime,
    current_session_start: OffsetDateTime,

    pts_offset: Option<PtsOffset>,
    subtitle_source: Option<SubtitleSource>,

    timeout: bool,

    last_progress: OffsetDateTime,
}

#[derive(Clone)]
struct Segment {
    path: String,
    duration: f64,
    program_date_time: OffsetDateTime,
}

pub struct PlaylistManagerOutputFiles {
    pub generated_playlist_file: String,
    pub ffmpeg_playlist_file: String,
    pub generated_subtitle_playlist_file: String,
}

impl PlaylistManager {
    pub fn new(
        channel_start_time: OffsetDateTime,
        target_duration: u32,
        output_folder: PathBuf,
        ready_file: PathBuf,
        output_files: PlaylistManagerOutputFiles,
    ) -> PlaylistManager {
        let heartbeat_file = output_folder.join(HEARTBEAT_FILE_NAME);

        PlaylistManager {
            output_folder,
            ready_file,
            heartbeat_file,
            generated_playlist_file: output_files.generated_playlist_file,
            ffmpeg_playlist_file: output_files.ffmpeg_playlist_file,
            generated_subtitle_playlist_file: output_files.generated_subtitle_playlist_file,
            ready: false,
            ready_at: None,

            segments: VecDeque::new(),
            discontinuity_before: HashSet::new(),
            media_sequence: 0,
            last_served_media_sequence: 0,
            discontinuity_sequence: 0,
            target_duration,
            target_duration_f64: target_duration as f64,
            pending_discontinuity: false,
            last_segment_end: channel_start_time,
            current_session_start: channel_start_time,

            pts_offset: None,
            subtitle_source: None,

            timeout: false,

            last_progress: OffsetDateTime::now_utc(),
        }
    }

    pub fn timeout(&self) -> &bool {
        &self.timeout
    }

    pub fn last_progress(&self) -> &OffsetDateTime {
        &self.last_progress
    }

    pub fn is_ready(&self) -> &bool {
        &self.ready
    }

    pub async fn before_new_pipeline(
        &mut self,
        scheduled_start: OffsetDateTime,
        new_pts_offset: Option<PtsOffset>,
        new_subtitle_source: Option<SubtitleSource>,
    ) -> Result<(), ChannelError> {
        self.update().await?;
        self.pts_offset = new_pts_offset;
        self.subtitle_source = new_subtitle_source;
        self.pending_discontinuity = true;

        // pacing follows the schedule, not segment output; a short source would leave
        // program date times behind for good, and new segments would be trimmed on arrival.
        // never move back: program date times must only increase
        if scheduled_start > self.last_segment_end {
            let lag = scheduled_start - self.last_segment_end;
            if lag > SHORTFALL_TOLERANCE {
                log::warn!(
                    "hls timeline is {:.3}s behind schedule; jumping ahead to {scheduled_start}",
                    lag.as_seconds_f64()
                );
            }
            self.last_segment_end = scheduled_start;
        }
        self.current_session_start = self.last_segment_end;

        self.last_progress = OffsetDateTime::now_utc();

        // overwrite ffmpeg's playlist with a generated playlist (containing *all* segments)
        if Path::new(&self.generated_playlist_file).exists() {
            let (generated_playlist, _) = self.generate_playlist(|s| s.to_owned(), None)?;
            write_atomic(
                &self.output_folder,
                &self.ffmpeg_playlist_file,
                generated_playlist,
            )
            .await?;
        }

        Ok(())
    }

    pub async fn pipeline_output(&mut self) -> Result<time::Duration, ChannelError> {
        self.update().await?;
        Ok(self.last_segment_end - self.current_session_start)
    }

    pub async fn update(&mut self) -> Result<(), ChannelError> {
        // scan for segments on disk
        let mut new_segment_files: VecDeque<String> = VecDeque::new();
        let mut entries = tokio::fs::read_dir(&self.output_folder)
            .await
            .io_context("scan the segment folder", &self.output_folder)?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(file_name) = entry.file_name().to_str()
                && file_name.ends_with(".ts")
                && !self.segments.iter().any(|s| s.path == file_name)
            {
                new_segment_files.push_back(file_name.to_owned());
            }
        }

        // get all segment durations from extinf tags in ffmpeg playlist
        let new_segment_durations: HashMap<String, f64> = self.get_new_segment_durations().await?;

        // filter out segments without a known duration
        let mut sorted_new_segments: Vec<String> = Vec::new();
        for segment in new_segment_files {
            if new_segment_durations.contains_key(&segment) {
                sorted_new_segments.push(segment);
            }
        }
        sorted_new_segments.sort();

        // add new segments
        for file in sorted_new_segments {
            let duration = new_segment_durations
                .get(&file)
                .map(|f| f.to_owned())
                .unwrap_or(self.target_duration_f64);

            // ffmpeg writes an empty segment after seeking past the end of the input
            if duration <= 0.0 {
                let path = self.output_folder.join(&file);
                tokio::fs::remove_file(&path)
                    .await
                    .io_context("delete the empty segment", &path)?;
                continue;
            }

            if self.pending_discontinuity {
                self.discontinuity_before.insert(file.to_owned());
                self.pending_discontinuity = false;
            }

            // rfc8216bis 6.2.1 requires EXT-X-TARGETDURATION to stay constant,
            // and 4.4.3.1 only requires it to cover segment durations rounded
            // to the nearest integer; raise it (a spec violation players
            // tolerate better than an undersized target) only when a segment
            // genuinely exceeds the rounding allowance
            if duration.round() > (self.target_duration as f64) {
                self.target_duration = duration.round() as u32;
            }

            let program_date_time = self.last_segment_end;

            self.segments.push_back(Segment {
                path: file.clone(),
                program_date_time,
                duration,
            });

            self.last_segment_end += Duration::from_secs_f64(duration);
            self.last_progress = OffsetDateTime::now_utc();

            let vtt_path = format!("{}.vtt", file.strip_suffix(".ts").unwrap_or(&file));
            let vtt_full = self.output_folder.join(&vtt_path);
            let stream_start = self.pts_offset.unwrap_or_default().duration
                + Duration::from_secs_f64(
                    (program_date_time - self.current_session_start)
                        .as_seconds_f64()
                        .max(0.0),
                );
            if let Some(src) = &mut self.subtitle_source {
                let body = render_subtitle_segment(
                    src,
                    src.next_segment_source_offset,
                    duration,
                    stream_start,
                );
                write_atomic(&self.output_folder, &vtt_full, body).await?;
                src.next_segment_source_offset += Duration::from_secs_f64(duration);
            } else {
                let body = format!(
                    "WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:{}\n\n",
                    (stream_start.as_secs_f64() * 90_000.0) as u64 % MPEGTS_WRAP
                );
                write_atomic(&self.output_folder, &vtt_full, body).await?;
            }
        }

        // trim old segments
        let cutoff = OffsetDateTime::now_utc() - Duration::from_mins(2);
        while !self.segments.is_empty() && self.segments[0].program_date_time < cutoff {
            if let Some(removed) = self.segments.remove(0) {
                self.media_sequence += 1;
                if self.discontinuity_before.contains(&removed.path) {
                    self.discontinuity_before.remove(&removed.path);
                    self.discontinuity_sequence += 1;
                }

                let path = self.output_folder.join(&removed.path);
                remove_trimmed(&path, "delete the trimmed segment").await?;

                let vtt_path = self.output_folder.join(format!(
                    "{}.vtt",
                    removed.path.strip_suffix(".ts").unwrap_or(&removed.path)
                ));
                remove_trimmed(&vtt_path, "delete the trimmed subtitle segment").await?;
            }
        }

        let playlist_segment_count = self.write_playlists(Some(10), false).await?;

        if !self.ready && playlist_segment_count >= MIN_SEGMENTS {
            tokio::fs::write(&self.ready_file, b"")
                .await
                .io_context("publish the ready signal", &self.ready_file)?;
            self.ready = true;
            self.ready_at = Some(Instant::now());
        }

        let idle = match tokio::fs::metadata(&self.heartbeat_file).await {
            Ok(metadata) => Some(
                metadata
                    .modified()
                    .io_context(
                        "read the modified time of the heartbeat file",
                        &self.heartbeat_file,
                    )?
                    .elapsed()
                    .unwrap_or(Duration::MAX),
            ),
            // no client ever arrived
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.ready_at.map(|t| t.elapsed())
            }
            Err(e) => return Err(e).io_context("stat the heartbeat file", &self.heartbeat_file),
        };
        self.timeout = idle.is_some_and(|i| i > HEARTBEAT_FILE_TIMEOUT);

        Ok(())
    }

    /// A session that stops (troubleshooting) must publish segments after the publish horizon.
    pub async fn finish(&mut self) -> Result<(), ChannelError> {
        self.update().await?;
        self.write_playlists(None, true).await?;
        Ok(())
    }

    async fn write_playlists(
        &mut self,
        max_segments: Option<usize>,
        end: bool,
    ) -> Result<usize, ChannelError> {
        let (mut generated_playlist, playlist_segment_count) =
            self.generate_playlist(|s| s.to_owned(), max_segments)?;
        let (mut generated_subtitle_playlist, _) = self.generate_playlist(
            |s| format!("{}.vtt", s.strip_suffix(".ts").unwrap_or(s)),
            max_segments,
        )?;

        if end {
            generated_playlist.push_str("#EXT-X-ENDLIST\n");
            generated_subtitle_playlist.push_str("#EXT-X-ENDLIST\n");
        }

        write_atomic(
            &self.output_folder,
            &self.generated_playlist_file,
            generated_playlist,
        )
        .await?;

        write_atomic(
            &self.output_folder,
            &self.generated_subtitle_playlist_file,
            generated_subtitle_playlist,
        )
        .await?;

        Ok(playlist_segment_count)
    }

    fn generate_playlist(
        &mut self,
        path_map: fn(&str) -> String,
        max_segments: Option<usize>,
    ) -> Result<(String, usize), ChannelError> {
        let mut playlist = String::new();
        playlist.push_str("#EXTM3U\n");
        playlist.push_str("#EXT-X-VERSION:6\n");
        playlist.push_str(&format!("#EXT-X-TARGETDURATION:{}\n", self.target_duration));

        let (skip, limit) = match max_segments {
            Some(max) => {
                let horizon = OffsetDateTime::now_utc() + PUBLISH_LEAD;

                // index one past the newest segment we want to publish
                let head = self
                    .segments
                    .iter()
                    .position(|s| s.program_date_time >= horizon)
                    .unwrap_or(self.segments.len());

                // monotonic clamp, in absolute media-sequence space
                let candidate_ms = self.media_sequence + head.saturating_sub(max) as u64;
                let clamped_ms = candidate_ms.max(self.last_served_media_sequence);
                self.last_served_media_sequence = clamped_ms;

                let skip = ((clamped_ms - self.media_sequence) as usize).min(head);
                (skip, head - skip)
            }
            None => (0, self.segments.len()),
        };
        let effective_media_sequence = self.media_sequence + skip as u64;
        let effective_discontinuity_sequence = self.discontinuity_sequence
            + self
                .segments
                .iter()
                .take(skip)
                .filter(|s| self.discontinuity_before.contains(&s.path))
                .count() as u64;

        playlist.push_str(&format!(
            "#EXT-X-MEDIA-SEQUENCE:{}\n",
            effective_media_sequence
        ));
        if effective_discontinuity_sequence > 0 {
            playlist.push_str(&format!(
                "#EXT-X-DISCONTINUITY-SEQUENCE:{}\n",
                effective_discontinuity_sequence
            ));
        }
        playlist.push_str("#EXT-X-INDEPENDENT-SEGMENTS\n");

        let format = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3][offset_hour sign:mandatory][offset_minute]"
        );

        for segment in self.segments.iter().skip(skip).take(limit) {
            if self.discontinuity_before.contains(&segment.path) {
                playlist.push_str("#EXT-X-DISCONTINUITY\n");
            }
            playlist.push_str(&format!("#EXTINF:{:.6},\n", segment.duration));
            playlist.push_str(&format!(
                "#EXT-X-PROGRAM-DATE-TIME:{}\n",
                segment.program_date_time.format(format)?
            ));
            playlist.push_str(&format!("{}\n", path_map(&segment.path)));
        }

        Ok((playlist, limit))
    }

    async fn get_new_segment_durations(&self) -> Result<HashMap<String, f64>, ChannelError> {
        let mut result: HashMap<String, f64> = HashMap::new();

        let path = Path::new(&self.ffmpeg_playlist_file);
        if path.exists() {
            let contents = tokio::fs::read_to_string(&path)
                .await
                .io_context("read the ffmpeg playlist", path)?;
            let mut lines = contents.lines();
            while let Some(line) = lines.next() {
                let Some(inf) = line.strip_prefix("#EXTINF:") else {
                    continue;
                };

                // tags such as EXT-X-PROGRAM-DATE-TIME (absent when troubleshooting) may precede the uri
                let Some(segment_name) = lines
                    .by_ref()
                    .find(|l| !l.is_empty() && !l.starts_with('#'))
                else {
                    break;
                };

                if segment_name.ends_with(".ts")
                    && let Some(Ok(duration)) = inf.split(',').next().map(str::parse::<f64>)
                {
                    result.insert(segment_name.to_owned(), duration);
                }
            }
        }

        Ok(result)
    }
}

async fn remove_trimmed(path: &Path, operation: &'static str) -> Result<(), ChannelError> {
    match tokio::fs::remove_file(path).await {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::warn!("trimmed segment {} was already gone", path.display());
            Ok(())
        }
        result => result.io_context(operation, path),
    }
}

async fn write_atomic(
    folder: &Path,
    dest: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> Result<(), ChannelError> {
    let dest = dest.as_ref();
    let temp =
        tempfile::NamedTempFile::new_in(folder).io_context("create a temp file for", dest)?;
    tokio::fs::write(temp.path(), contents)
        .await
        .io_context("write a temp file for", dest)?;
    tokio::fs::rename(temp.path(), dest)
        .await
        .io_context("rename a temp file onto", dest)
}

fn render_subtitle_segment(
    src: &mut SubtitleSource,
    seg_start_src: Duration,
    duration: f64,
    stream_start: Duration,
) -> String {
    let seg_end_src = (seg_start_src + Duration::from_secs_f64(duration)).min(src.out_point);

    // rfc8216 3.5 requires the same cue times in every segment, and clients drop only exact
    // duplicates. Fix the offset once per pass, so ms rounding can't change between segments.
    // Cut cues at pass edges: each pass starts after a discontinuity, and hls.js moves each
    // discontinuity by a few ms, so a cue that spans one is shown twice.
    let stream_offset_ms = *src
        .stream_offset_ms
        .get_or_insert(stream_start.as_millis() as i64 - seg_start_src.as_millis() as i64);
    let stream_ms = |t: Duration| (t.as_millis() as i64 + stream_offset_ms).max(0) as u64;

    let mut cues = Vec::new();
    let mut segment_cursor = src.cursor;

    while let Some(cue) = src.cues.get(segment_cursor)
        && cue.start < seg_end_src
    {
        let start = cue.start.max(src.in_point);
        let end = cue.end.min(src.out_point);
        if end > seg_start_src {
            cues.push((stream_ms(start), stream_ms(end), cue));
        }

        // walk persistent cursor if this cue will never display again
        if src.cursor == segment_cursor && end <= seg_end_src {
            src.cursor += 1;
        }

        segment_cursor += 1;
    }

    // cue times can't be negative, so a cue that started before the grid line moves the anchor
    let grid_ms = stream_start.as_secs() / SUBTITLE_ANCHOR_INTERVAL_SECS
        * SUBTITLE_ANCHOR_INTERVAL_SECS
        * 1000;
    let anchor_ms = cues
        .iter()
        .map(|(start, _, _)| *start)
        .fold(grid_ms, u64::min);

    let mut out = format!(
        "WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:{}\n\n",
        (anchor_ms * 90 + SUBTITLE_ANCHOR_NUDGE_TICKS) % MPEGTS_WRAP
    );

    for (start, end, cue) in cues {
        let settings = if cue.settings.is_empty() {
            String::new()
        } else {
            format!(" {}", cue.settings)
        };
        out.push_str(&format!(
            "{} --> {}{}\n{}\n\n",
            format_vtt_ts(Duration::from_millis(start - anchor_ms)),
            format_vtt_ts(Duration::from_millis(end - anchor_ms)),
            settings,
            cue.text
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playlist_manager(folder: &Path, channel_start_time: OffsetDateTime) -> PlaylistManager {
        let file = |name: &str| folder.join(name).to_string_lossy().into_owned();
        PlaylistManager::new(
            channel_start_time,
            ffpipeline::pipeline::SEGMENT_SECONDS,
            folder.to_path_buf(),
            folder.join(ersatztv_core::READY_FILE_NAME),
            PlaylistManagerOutputFiles {
                generated_playlist_file: file("live.m3u8"),
                ffmpeg_playlist_file: file("ffmpeg.m3u8"),
                generated_subtitle_playlist_file: file("live_sub.m3u8"),
            },
        )
    }

    async fn write_ffmpeg_segments(folder: &Path, names: &[&str]) {
        let mut playlist = String::from("#EXTM3U\n");
        for name in names {
            tokio::fs::write(folder.join(name), b"").await.unwrap();
            playlist.push_str(&format!(
                "#EXTINF:4.000000,\n#EXT-X-PROGRAM-DATE-TIME:2026-01-01T00:00:00.000+0000\n{name}\n"
            ));
        }
        tokio::fs::write(folder.join("ffmpeg.m3u8"), playlist)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn idle_clock_starts_at_ready_without_heartbeat_file() {
        let folder = tempfile::tempdir().unwrap();
        let now = OffsetDateTime::now_utc();

        let mut pm = playlist_manager(folder.path(), now - Duration::from_mins(1));
        pm.update().await.unwrap();
        assert!(!pm.ready);
        assert!(!pm.timeout);

        let names = [
            "live000000.ts",
            "live000001.ts",
            "live000002.ts",
            "live000003.ts",
        ];
        write_ffmpeg_segments(folder.path(), &names).await;
        pm.update().await.unwrap();
        assert!(pm.ready);
        assert!(!pm.timeout);

        pm.ready_at = Instant::now().checked_sub(HEARTBEAT_FILE_TIMEOUT + Duration::from_secs(1));
        pm.update().await.unwrap();
        assert!(pm.timeout);

        tokio::fs::write(folder.path().join(HEARTBEAT_FILE_NAME), b"")
            .await
            .unwrap();
        pm.update().await.unwrap();
        assert!(!pm.timeout);
    }

    #[tokio::test]
    async fn segments_after_shortfall_are_not_trimmed_on_arrival() {
        let folder = tempfile::tempdir().unwrap();
        let now = OffsetDateTime::now_utc();

        // 10 minute shortfall
        let mut pm = playlist_manager(folder.path(), now - Duration::from_mins(10));
        pm.before_new_pipeline(now - Duration::from_secs(8), None, None)
            .await
            .unwrap();

        let names = ["live000000.ts", "live000001.ts", "live000002.ts"];
        write_ffmpeg_segments(folder.path(), &names).await;
        pm.update().await.unwrap();

        assert_eq!(pm.segments.len(), names.len());
        assert_eq!(pm.media_sequence, 0);
        let live = tokio::fs::read_to_string(folder.path().join("live.m3u8"))
            .await
            .unwrap();
        for name in names {
            assert!(folder.path().join(name).exists());
            assert!(live.contains(name));
        }
    }

    #[tokio::test]
    async fn finish_publishes_past_the_horizon_and_ends_playlists() {
        let folder = tempfile::tempdir().unwrap();
        let now = OffsetDateTime::now_utc();

        let mut pm = playlist_manager(folder.path(), now);
        pm.before_new_pipeline(now, None, None).await.unwrap();

        // longer than the publish horizon
        let names: Vec<String> = (0..8).map(|i| format!("live{i:06}.ts")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        write_ffmpeg_segments(folder.path(), &names).await;

        pm.update().await.unwrap();
        let live = tokio::fs::read_to_string(folder.path().join("live.m3u8"))
            .await
            .unwrap();
        assert!(!live.contains(names[7]));

        pm.finish().await.unwrap();
        for file in ["live.m3u8", "live_sub.m3u8"] {
            let playlist = tokio::fs::read_to_string(folder.path().join(file))
                .await
                .unwrap();
            assert_eq!(playlist.matches("#EXTINF").count(), names.len());
            assert!(playlist.ends_with("#EXT-X-ENDLIST\n"));
        }
    }

    #[tokio::test]
    async fn empty_segments_are_dropped() {
        let folder = tempfile::tempdir().unwrap();
        let now = OffsetDateTime::now_utc();

        let mut pm = playlist_manager(folder.path(), now);
        pm.before_new_pipeline(now, None, None).await.unwrap();

        tokio::fs::write(folder.path().join("live000000.ts"), b"")
            .await
            .unwrap();
        tokio::fs::write(
            folder.path().join("ffmpeg.m3u8"),
            "#EXTM3U\n#EXTINF:0.000000,\n#EXT-X-PROGRAM-DATE-TIME:2026-01-01T00:00:00.000+0000\nlive000000.ts\n",
        )
        .await
        .unwrap();

        assert_eq!(pm.pipeline_output().await.unwrap(), time::Duration::ZERO);
        assert!(pm.segments.is_empty());
        assert!(!folder.path().join("live000000.ts").exists());
    }

    #[tokio::test]
    async fn pipeline_output_counts_only_the_current_pipeline() {
        let folder = tempfile::tempdir().unwrap();
        let now = OffsetDateTime::now_utc();

        let mut pm = playlist_manager(folder.path(), now - Duration::from_secs(8));
        write_ffmpeg_segments(folder.path(), &["live000000.ts"]).await;
        pm.update().await.unwrap();

        pm.before_new_pipeline(now, None, None).await.unwrap();
        write_ffmpeg_segments(folder.path(), &["live000001.ts", "live000002.ts"]).await;

        assert_eq!(
            pm.pipeline_output().await.unwrap(),
            time::Duration::seconds(8)
        );
    }

    #[tokio::test]
    async fn segments_without_program_date_time_are_counted() {
        let folder = tempfile::tempdir().unwrap();
        let now = OffsetDateTime::now_utc();

        let mut pm = playlist_manager(folder.path(), now);
        pm.before_new_pipeline(now, None, None).await.unwrap();

        // troubleshooting omits the program_date_time hls flag
        for name in ["live000000.ts", "live000001.ts"] {
            tokio::fs::write(folder.path().join(name), b"")
                .await
                .unwrap();
        }
        tokio::fs::write(
            folder.path().join("ffmpeg.m3u8"),
            "#EXTM3U\n#EXT-X-DISCONTINUITY\n#EXTINF:4.004000,\nlive000000.ts\n#EXTINF:3.128333,\nlive000001.ts\n#EXT-X-ENDLIST\n",
        )
        .await
        .unwrap();

        assert_eq!(
            pm.pipeline_output().await.unwrap(),
            time::Duration::seconds_f64(4.004 + 3.128333)
        );
    }

    #[tokio::test]
    async fn timeline_ahead_of_schedule_is_not_moved_back() {
        let folder = tempfile::tempdir().unwrap();
        let now = OffsetDateTime::now_utc();

        let mut pm = playlist_manager(folder.path(), now);
        pm.before_new_pipeline(now - Duration::from_mins(1), None, None)
            .await
            .unwrap();

        assert_eq!(pm.last_segment_end, now);
        assert_eq!(pm.current_session_start, now);
    }

    fn segment(path: &str, start_offset_secs: i64) -> Segment {
        Segment {
            path: path.to_owned(),
            duration: 4.0,
            program_date_time: OffsetDateTime::UNIX_EPOCH
                + Duration::from_secs(start_offset_secs as u64),
        }
    }

    #[tokio::test]
    async fn scanning_a_missing_segment_folder_names_the_folder() {
        let folder = tempfile::tempdir().unwrap();
        let missing = folder.path().join("gone");
        let mut manager = playlist_manager(&missing, OffsetDateTime::UNIX_EPOCH);

        let message = manager.update().await.unwrap_err().to_string();

        assert!(
            message.contains("scan the segment folder"),
            "message does not name the operation: {message}"
        );
        assert!(
            message.contains(&missing.display().to_string()),
            "message does not name the folder: {message}"
        );
        assert!(
            !message.contains("channel config"),
            "message blames the channel config: {message}"
        );
    }

    #[tokio::test]
    async fn trimming_a_segment_whose_file_is_gone_succeeds() {
        let folder = tempfile::tempdir().unwrap();
        let mut manager = playlist_manager(folder.path(), OffsetDateTime::UNIX_EPOCH);
        manager.segments.push_back(segment("live000042.ts", 0));

        manager.update().await.unwrap();

        assert!(manager.segments.is_empty());
    }

    #[tokio::test]
    async fn trimming_a_segment_that_cannot_be_deleted_names_the_segment() {
        let folder = tempfile::tempdir().unwrap();
        let mut manager = playlist_manager(folder.path(), OffsetDateTime::UNIX_EPOCH);
        // remove_file fails on a directory, but not with NotFound
        tokio::fs::create_dir(folder.path().join("live000042.ts"))
            .await
            .unwrap();
        manager.segments.push_back(segment("live000042.ts", 0));

        let message = manager.update().await.unwrap_err().to_string();

        assert!(
            message.contains("delete the trimmed segment"),
            "message does not name the operation: {message}"
        );
        assert!(
            message.contains("live000042.ts"),
            "message does not name the segment: {message}"
        );
        assert!(
            !message.contains("channel config"),
            "message blames the channel config: {message}"
        );
    }

    fn cue(start: f64, end: f64, text: &str) -> Cue {
        Cue {
            start: Duration::from_secs_f64(start),
            end: Duration::from_secs_f64(end),
            settings: String::new(),
            text: text.to_owned(),
        }
    }

    fn subtitle_source(cues: Vec<Cue>, in_point: f64, out_point: f64) -> SubtitleSource {
        SubtitleSource::new(
            Arc::new(cues),
            Duration::from_secs_f64(in_point),
            Duration::from_secs_f64(out_point),
        )
    }

    fn render_segments(src: &mut SubtitleSource, stream_start: f64, count: u64) -> Vec<String> {
        (0..count)
            .map(|i| {
                let start = src.next_segment_source_offset;
                src.next_segment_source_offset += Duration::from_secs(4);
                let stream_start = Duration::from_secs_f64(stream_start + 4.0 * i as f64);
                render_subtitle_segment(src, start, 4.0, stream_start)
            })
            .collect()
    }

    #[test]
    fn spanning_cue_keeps_its_times_in_every_segment() {
        let mut src = subtitle_source(vec![cue(2.0, 11.0, "long")], 0.0, 60.0);

        let segments = render_segments(&mut src, 0.0, 3);

        for segment in &segments {
            assert!(
                segment.starts_with("WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:1\n"),
                "{segment}"
            );
            assert!(
                segment.contains("00:00:02.000 --> 00:00:11.000\nlong"),
                "{segment}"
            );
        }
    }

    #[test]
    fn cue_active_at_the_stream_start_starts_with_the_stream() {
        let mut src = subtitle_source(vec![cue(98.0, 106.0, "joined")], 100.0, 400.0);

        let segments = render_segments(&mut src, 0.0, 3);

        assert!(segments[0].contains("00:00:00.000 --> 00:00:06.000\njoined"));
        assert!(segments[1].contains("00:00:00.000 --> 00:00:06.000\njoined"));
        assert!(!segments[2].contains("joined"));
    }

    #[test]
    fn cue_ending_on_a_boundary_is_not_repeated() {
        let mut src = subtitle_source(
            vec![cue(1.0, 4.0, "first"), cue(4.0, 6.0, "second")],
            0.0,
            60.0,
        );

        let segments = render_segments(&mut src, 0.0, 2);

        assert!(segments[0].contains("first"));
        assert!(!segments[0].contains("second"));
        assert!(!segments[1].contains("first"));
        assert!(segments[1].contains("00:00:04.000 --> 00:00:06.000\nsecond"));
    }

    #[test]
    fn cues_are_clamped_to_the_pass_end() {
        let mut src = subtitle_source(
            vec![
                cue(5.0, 9.0, "crosses the end"),
                cue(8.5, 9.5, "after the end"),
            ],
            0.0,
            7.5,
        );

        let segments = render_segments(&mut src, 0.0, 3);

        assert!(segments[1].contains("00:00:05.000 --> 00:00:07.500\ncrosses the end"));
        assert!(segments.iter().all(|s| !s.contains("after the end")));
        assert!(!segments[2].contains("crosses the end"));
    }

    #[test]
    fn cues_are_cut_at_pass_edges() {
        let cues = vec![cue(42.0, 46.0, "split")];
        let mut first_pass = subtitle_source(cues.clone(), 0.0, 44.0);
        let mut second_pass = subtitle_source(cues, 44.0, 60.0);

        let first = render_segments(&mut first_pass, 0.0, 11);
        // after a seek, a pass can start 21 ms off the previous pass
        let second = render_segments(&mut second_pass, 44.021, 1);

        assert!(first[10].contains("00:00:42.000 --> 00:00:44.000\nsplit"));
        assert!(second[0].contains("00:00:44.021 --> 00:00:46.021\nsplit"));
    }

    #[test]
    fn cue_crossing_a_grid_line_keeps_its_absolute_times() {
        let mut src = subtitle_source(vec![cue(2.0, 7.0, "anchored")], 0.0, 60.0);

        let segments = render_segments(&mut src, 1196.0, 2);

        assert!(segments[0].contains(&format!("MPEGTS:{}\n", 600 * 90_000 + 1)));
        assert!(segments[0].contains("00:09:58.000 --> 00:10:03.000\nanchored"));
        assert!(segments[1].contains(&format!("MPEGTS:{}\n", 1198 * 90_000 + 1)));
        assert!(segments[1].contains("00:00:00.000 --> 00:00:05.000\nanchored"));
    }

    #[test]
    fn long_cue_crossing_a_grid_line_keeps_its_start() {
        let mut src = subtitle_source(vec![cue(500.0, 620.0, "long")], 0.0, 700.0);
        src.next_segment_source_offset = Duration::from_secs(596);

        let segments = render_segments(&mut src, 596.0, 2);

        assert!(segments[0].contains("MPEGTS:1\n"));
        assert!(segments[0].contains("00:08:20.000 --> 00:10:20.000\nlong"));
        assert!(segments[1].contains(&format!("MPEGTS:{}\n", 500 * 90_000 + 1)));
        assert!(segments[1].contains("00:00:00.000 --> 00:02:00.000\nlong"));
    }

    #[test]
    fn anchor_wraps_with_the_mpegts_clock() {
        let mut src = subtitle_source(vec![cue(1.0, 2.0, "late")], 0.0, 60.0);

        let segments = render_segments(&mut src, 100_000.0, 1);

        assert!(segments[0].contains(&format!("MPEGTS:{}\n", (99_600 * 90_000 + 1) % MPEGTS_WRAP)));
        assert!(segments[0].contains("00:06:41.000 --> 00:06:42.000\nlate"));
    }
}
