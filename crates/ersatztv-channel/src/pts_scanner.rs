use std::path::{Path, PathBuf};
use std::time::Duration;

use ersatztv_channel::config::ChannelConfig;
use ersatztv_channel::error::{ChannelError, IoContext};
use ersatztv_core::process;
use ffpipeline::pipeline::PtsOffset;

const MPEGTS_TICKS_PER_SECOND: u64 = 90_000;

pub struct PtsScanner {
    output_folder: PathBuf,
    ffprobe_path: PathBuf,
}

impl PtsScanner {
    pub fn new(channel_config: &ChannelConfig, ffprobe_path: &Path) -> PtsScanner {
        PtsScanner {
            output_folder: channel_config.expanded_output_folder().to_owned(),
            ffprobe_path: ffprobe_path.to_owned(),
        }
    }

    pub async fn get_last_pts(&self) -> Result<PtsOffset, ChannelError> {
        let mut pts_time = PtsOffset::default();

        // find last segment file in output folder
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&self.output_folder).await.io_context(
            "scan the output folder for the last segment",
            &self.output_folder,
        )?;
        while let Ok(Some(entry)) = dir.next_entry().await {
            if entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
            {
                entries.push(entry);
            }
        }
        entries.sort_by_key(|a| std::cmp::Reverse(a.file_name()));
        if let Some(last_segment) = entries.first() {
            // call ffprobe
            let path = last_segment
                .path()
                .into_os_string()
                .into_string()
                .map_err(|p| {
                    ChannelError::PtsScannerPathNotUtf8(p.to_string_lossy().into_owned())
                })?;

            let output = process::command(&self.ffprobe_path)
                .args([
                    "-v",
                    "-0",
                    "-show_entries",
                    "packet=codec_type,pts,duration",
                    "-of",
                    "compact=p=0:nk=1",
                    &path,
                ])
                .output()
                .await
                .io_context("run ffprobe on the last segment", &path)?;

            pts_time = parse_stream_ends(&String::from_utf8_lossy(&output.stdout));
        }

        Ok(pts_time)
    }
}

fn parse_stream_ends(stdout: &str) -> PtsOffset {
    let mut video_end = 0;
    let mut audio_end = None;
    for line in stdout.lines() {
        let mut fields = line.trim().split('|');
        let (Some(codec_type), Some(Ok(pts))) =
            (fields.next(), fields.next().map(str::parse::<u64>))
        else {
            continue;
        };
        let end = pts
            + fields
                .next()
                .and_then(|d| d.parse::<u64>().ok())
                .unwrap_or(0);
        match codec_type {
            "video" => video_end = video_end.max(end),
            "audio" => audio_end = Some(audio_end.unwrap_or(0).max(end)),
            _ => {}
        }
    }

    PtsOffset {
        duration: ticks_to_duration(video_end),
        audio_end: audio_end.map(ticks_to_duration),
    }
}

fn ticks_to_duration(ticks: u64) -> Duration {
    Duration::from_nanos((ticks * 1_000_000_000).div_ceil(MPEGTS_TICKS_PER_SECOND))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_audio_past_the_last_video_packet() {
        let ends = parse_stream_ends(
            "video|430800|3000\naudio|433680|1920\naudio|435600|1920\nvideo|432600|3000\n",
        );

        assert_eq!(ends.duration, ticks_to_duration(435600));
        assert_eq!(ends.audio_end, Some(ticks_to_duration(437520)));
    }

    #[test]
    fn uses_the_latest_end_rather_than_the_last_packet() {
        // B-frames: packets are not in pts order
        let ends = parse_stream_ends("video|9000|3000\nvideo|3000|3000\n");

        assert_eq!(ends.duration, ticks_to_duration(12000));
        assert_eq!(ends.audio_end, None);
    }

    #[test]
    fn skips_packets_without_a_pts() {
        let ends = parse_stream_ends("video|6000|3000\nvideo|N/A|3000\naudio|N/A|1920\n");

        assert_eq!(ends.duration, ticks_to_duration(9000));
        assert_eq!(ends.audio_end, None);
    }

    #[test]
    fn rounds_ticks_up() {
        assert_eq!(ticks_to_duration(1), Duration::from_nanos(11_112));
    }
}
