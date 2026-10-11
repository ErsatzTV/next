//! Stream copy can only start on a keyframe, and `-ss` lands on a different keyframe per demuxer
//! (mp4/mkv: the one before, mpegts: the one after). Both make the HLS timeline drift. So a copy
//! starts on the keyframe at or before the target and ends before the next chunk's keyframe.

use std::path::Path;
use std::time::Duration;

use ersatztv_core::process;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::copy_decision::CopyBlocker;
use crate::error::FFPipelineError;
use crate::input::{FfmpegInputArgs, ProbedInput};

/// Output `-ss` drops packets by dts; this puts it just below the keyframe's dts.
const START_EPSILON: Duration = Duration::from_micros(500);
/// Far enough that every demuxer lands before the keyframe.
const INPUT_SEEK_MARGIN: Duration = Duration::from_secs(1);
const LANDING_TOLERANCE: Duration = Duration::from_millis(1);
const MAX_BACK_OFFS: usize = 9;
/// Limits dry-run reads. Without a keyframe this close, copy would drop the video anyway.
const KEYFRAME_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keyframe {
    /// Relative to the input start time, like `-ss`.
    pub time: Duration,
    pub reorder_delay: Duration,
}

impl Keyframe {
    fn dts(&self) -> Duration {
        self.time.saturating_sub(self.reorder_delay)
    }
}

/// `None`: start at the input start, or run for the scheduled duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopySeek {
    pub start: Option<Keyframe>,
    pub end: Option<Keyframe>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CopySeekArgs {
    pub(crate) input_seek: Duration,
    pub(crate) output_seek: Option<Duration>,
    pub(crate) duration: Duration,
    /// The keyframe's output pts is its reorder delay, not 0.
    pub(crate) ts_offset_correction: Duration,
}

impl CopySeek {
    /// The input `-ss` is coarse. The output `-ss` is exact because it drops packets by dts.
    /// `-t` also stops on dts, so a chunk ends half a frame before the end keyframe's dts.
    pub(crate) fn args(&self, scheduled: Duration, frame_duration: Duration) -> CopySeekArgs {
        let start_dts = self.start.map(|k| k.dts().saturating_sub(START_EPSILON));
        let input_seek = start_dts
            .map(|dts| {
                let seek = dts.saturating_sub(INPUT_SEEK_MARGIN);
                Duration::from_millis(seek.as_millis() as u64)
            })
            .unwrap_or_default();
        let output_seek = start_dts.map(|dts| dts - input_seek);
        let duration = match self.end {
            Some(end) => end
                .dts()
                .saturating_sub(frame_duration / 2)
                .saturating_sub(start_dts.unwrap_or_default()),
            None => scheduled,
        };

        CopySeekArgs {
            input_seek,
            output_seek,
            duration,
            ts_offset_correction: self.start.map(|k| k.reorder_delay).unwrap_or_default(),
        }
    }
}

/// Uses ffmpeg dry runs on the same demux path as the copy. ffprobe `-read_intervals` can't:
/// on mpegts its first packet isn't a keyframe.
pub struct KeyframeLocator<'a> {
    ffmpeg_path: &'a Path,
    input: &'a ProbedInput,
    video_index: u32,
    audio_index: Option<u32>,
}

impl<'a> KeyframeLocator<'a> {
    /// Maps the same streams as the pipeline. Near the start of an mpegts input, ffmpeg counts
    /// time from the earliest mapped stream (`correct_input_start_times`).
    pub fn new(
        ffmpeg_path: &'a Path,
        input: &'a ProbedInput,
        video_index: u32,
        audio_index: Option<u32>,
    ) -> Self {
        Self {
            ffmpeg_path,
            input,
            video_index,
            audio_index,
        }
    }

    fn dry_run_command(&self, seek: Duration, limit: &[&str]) -> Command {
        let mut command = process::command(self.ffmpeg_path);
        command.args(["-nostdin", "-hide_banner", "-v", "error"]);
        command.args(
            self.input
                .input_source
                .args_for_input()
                .iter()
                .map(|a| a.as_ref()),
        );
        if !seek.is_zero() {
            command.args(["-ss", &format!("{:.6}", seek.as_secs_f64())]);
        }
        command
            .args(["-i", &self.input.probe_result.path])
            .args(["-map", &format!("0:{}", self.video_index)]);
        if let Some(audio_index) = self.audio_index {
            command.args(["-map", &format!("0:{audio_index}")]);
        }
        command
            .args(["-c", "copy"])
            .args(limit)
            .args(["-f", "framecrc", "-"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        command
    }

    async fn dry_run(&self, seek: Duration, limit: &[&str]) -> Result<String, FFPipelineError> {
        let output = self
            .dry_run_command(seek, limit)
            .output()
            .await
            .map_err(|e| FFPipelineError::KeyframeSearchFailed(e.to_string()))?;

        if !output.status.success() {
            return Err(FFPipelineError::KeyframeSearchFailed(format!(
                "ffmpeg exited with {}",
                output.status
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// A file cut between keyframes starts with packets that copy drops, so the video starts
    /// late and the HLS timeline is short. `None`: no video packets.
    pub async fn input_start(&self) -> Result<Option<InputStart>, FFPipelineError> {
        let window = format!("{:.6}", KEYFRAME_WINDOW.as_secs_f64());
        let mut child = self
            .dry_run_command(Duration::ZERO, &["-copyinkf", "-t", &window])
            .spawn()
            .map_err(|e| FFPipelineError::KeyframeSearchFailed(e.to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| FFPipelineError::KeyframeSearchFailed(String::from("no stdout")))?;

        let mut lines = BufReader::new(stdout).lines();
        let mut parser = FramecrcParser::new(Duration::ZERO);
        let mut first_packet_is_keyframe = None;
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| FFPipelineError::KeyframeSearchFailed(e.to_string()))?
        {
            let Some(packet) = parser.line(&line) else {
                continue;
            };
            first_packet_is_keyframe.get_or_insert(packet.is_keyframe);
            if packet.is_keyframe {
                // dropping the child kills ffmpeg
                return Ok(Some(InputStart {
                    first_packet_is_keyframe: first_packet_is_keyframe == Some(true),
                    first_keyframe: Some(packet.keyframe),
                }));
            }
        }

        let status = child
            .wait()
            .await
            .map_err(|e| FFPipelineError::KeyframeSearchFailed(e.to_string()))?;
        if !status.success() {
            return Err(FFPipelineError::KeyframeSearchFailed(format!(
                "ffmpeg exited with {status}"
            )));
        }
        Ok(first_packet_is_keyframe.map(|_| InputStart {
            first_packet_is_keyframe: false,
            first_keyframe: None,
        }))
    }

    pub async fn land(&self, seek: Duration) -> Result<Option<Keyframe>, FFPipelineError> {
        let window = format!("{:.6}", KEYFRAME_WINDOW.as_secs_f64());
        let stdout = self
            .dry_run(seek, &["-frames:v", "1", "-t", &window])
            .await?;
        Ok(parse_framecrc(&stdout, seek)
            .into_iter()
            .next()
            .map(|p| p.keyframe))
    }

    /// mpegts lands on the keyframe after `target`, so search back for the last one before it.
    pub async fn at_or_before(&self, target: Duration) -> Result<Keyframe, FFPipelineError> {
        if let Some(landing) = self.land(target).await?
            && (landing.time <= target + LANDING_TOLERANCE || target.is_zero())
        {
            return Ok(landing);
        }

        let mut back_off = Duration::from_secs(1);
        for _ in 0..MAX_BACK_OFFS {
            let seek = target.saturating_sub(back_off);
            let window = format!("{:.6}", (target - seek + LANDING_TOLERANCE).as_secs_f64());
            let stdout = self.dry_run(seek, &["-t", &window]).await?;
            if let Some(keyframe) = parse_framecrc(&stdout, seek)
                .into_iter()
                .rev()
                .filter(|p| p.is_keyframe && p.keyframe.time <= target + LANDING_TOLERANCE)
                .map(|p| p.keyframe)
                .next()
            {
                return Ok(keyframe);
            }
            if seek.is_zero() {
                break;
            }
            back_off *= 2;
        }

        Err(FFPipelineError::NoKeyframe(format!(
            "at or before {target:?}"
        )))
    }

    pub async fn at_or_after(&self, target: Duration) -> Result<Keyframe, FFPipelineError> {
        let window = format!("{:.6}", KEYFRAME_WINDOW.as_secs_f64());
        let stdout = self.dry_run(target, &["-t", &window]).await?;
        parse_framecrc(&stdout, target)
            .into_iter()
            .filter(|p| p.is_keyframe)
            .map(|p| p.keyframe)
            .find(|k| k.time + LANDING_TOLERANCE >= target)
            .ok_or_else(|| {
                FFPipelineError::NoKeyframe(format!("within {KEYFRAME_WINDOW:?} after {target:?}"))
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputStart {
    pub first_packet_is_keyframe: bool,
    /// `None`: none within `KEYFRAME_WINDOW`
    pub first_keyframe: Option<Keyframe>,
}

impl InputStart {
    pub fn copy_blocker(&self) -> Option<CopyBlocker> {
        match (self.first_packet_is_keyframe, self.first_keyframe) {
            (_, None) => Some(CopyBlocker::NoKeyframes),
            (false, Some(_)) => Some(CopyBlocker::StartsBetweenKeyframes),
            (true, Some(_)) => None,
        }
    }
}

struct FramecrcPacket {
    keyframe: Keyframe,
    is_keyframe: bool,
}

/// Without `-copyts`, output timestamps count from the seek target, so source time is
/// `seek + pts`. `F=` is only written when the flags aren't exactly "keyframe".
struct FramecrcParser {
    seek: Duration,
    time_base: Option<f64>,
}

impl FramecrcParser {
    fn new(seek: Duration) -> Self {
        Self {
            seek,
            time_base: None,
        }
    }

    fn line(&mut self, line: &str) -> Option<FramecrcPacket> {
        if let Some(tb) = line.strip_prefix("#tb 0: ") {
            self.time_base = tb
                .split_once('/')
                .and_then(|(n, d)| {
                    Some((n.trim().parse::<f64>().ok()?, d.trim().parse::<f64>().ok()?))
                })
                .map(|(n, d)| n / d);
            return None;
        }
        if line.starts_with('#') {
            return None;
        }
        let tb = self.time_base?;
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        if fields.first() != Some(&"0") {
            return None;
        }
        let dts = fields.get(1)?.parse::<i64>().ok()?;
        let pts = fields.get(2)?.parse::<i64>().ok()?;
        let dts = (dts != i64::MIN).then_some(dts);
        let pts = (pts != i64::MIN).then_some(pts).or(dts)?;
        let flags = fields
            .iter()
            .find_map(|f| f.strip_prefix("F=0x"))
            .and_then(|f| u32::from_str_radix(f, 16).ok());
        let time = self.seek.as_secs_f64() + pts as f64 * tb;
        let reorder_delay = dts.map(|dts| (pts - dts) as f64 * tb).unwrap_or(0.0);
        Some(FramecrcPacket {
            keyframe: Keyframe {
                time: Duration::from_secs_f64(time.max(0.0)),
                reorder_delay: Duration::from_secs_f64(reorder_delay.max(0.0)),
            },
            is_keyframe: flags.is_none_or(|f| f & 1 == 1),
        })
    }
}

fn parse_framecrc(stdout: &str, seek: Duration) -> Vec<FramecrcPacket> {
    let mut parser = FramecrcParser::new(seek);
    stdout
        .lines()
        .filter_map(|line| parser.line(line))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CopySeekRequest {
    pub target: Duration,
    /// The copy must not start before the item's in-point.
    pub floor: Duration,
    pub remaining: Duration,
    /// `None`: run to the end of the item
    pub limit: Option<Duration>,
    pub resume: Option<Keyframe>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CopySeekPlan {
    pub seek: CopySeek,
    pub in_point: Duration,
    /// Schedule time, not the `-t` value
    pub duration: Duration,
    pub is_complete: bool,
}

pub async fn plan_copy_seek(
    locator: &KeyframeLocator<'_>,
    request: CopySeekRequest,
) -> Result<CopySeekPlan, FFPipelineError> {
    let start = match request.resume {
        Some(keyframe) => Some(keyframe),
        None if request.target.is_zero() => None,
        None => {
            let keyframe = locator.at_or_before(request.target).await?;
            if keyframe.time + LANDING_TOLERANCE < request.floor {
                Some(locator.at_or_after(request.floor).await?)
            } else {
                Some(keyframe)
            }
        }
    }
    // its dts is at or before 0, so no output `-ss` fits below it; play from the start
    .filter(|k| k.dts() > START_EPSILON);
    let in_point = start.map(|k| k.time).unwrap_or_default();

    if let Some(limit) = request.limit
        && request.remaining > limit
    {
        let end = locator.at_or_before(in_point + limit).await?;
        if end.time > in_point + LANDING_TOLERANCE {
            return Ok(CopySeekPlan {
                seek: CopySeek {
                    start,
                    end: Some(end),
                },
                in_point,
                duration: end.time - in_point,
                is_complete: false,
            });
        }
    }

    Ok(CopySeekPlan {
        seek: CopySeek { start, end: None },
        in_point,
        duration: request.remaining,
        is_complete: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_nanos(33_366_667);

    fn keyframe(time_ms: u64, delay_ms: u64) -> Keyframe {
        Keyframe {
            time: Duration::from_millis(time_ms),
            reorder_delay: Duration::from_millis(delay_ms),
        }
    }

    #[test]
    fn parses_landing_relative_to_seek() {
        // mp4: a seek to 13 s lands on the keyframe at 10.01 s
        let stdout = "#tb 0: 1/30000\n#media_type 0: video\n0,     -91702,     -89700,     1001,     9371, 0xd8e724ff\n";
        let packets = parse_framecrc(stdout, Duration::from_secs(13));
        let k = packets[0].keyframe;
        assert!((k.time.as_secs_f64() - 10.01).abs() < 1e-6);
        assert!((k.reorder_delay.as_secs_f64() - 2002.0 / 30000.0).abs() < 1e-6);
        assert!(packets[0].is_keyframe);
    }

    #[test]
    fn parses_flags_and_side_data() {
        let stdout = "#tb 0: 1/90000\n#tb 1: 1/90000\n\
            1,     -41520,     -41520,     1920,       51, 0xa1fc1961\n\
            0,     627714,     633720,     3003,     9575, 0x2c96783a, S=1, MPEGTS Stream ID,        1, 0x00e000e0\n\
            0,     630717,     645732,     3003,      120, 0x11111111, F=0x0\n";
        let packets = parse_framecrc(stdout, Duration::from_secs(13));
        assert!(packets[0].is_keyframe);
        assert!(
            (packets[0].keyframe.time.as_secs_f64() - (13.0 + 633720.0 / 90000.0)).abs() < 1e-6
        );
        assert!(!packets[1].is_keyframe);
    }

    #[test]
    fn input_start_blockers() {
        let start = |first_packet_is_keyframe, first_keyframe| InputStart {
            first_packet_is_keyframe,
            first_keyframe,
        };
        assert_eq!(start(true, Some(keyframe(0, 67))).copy_blocker(), None);
        assert_eq!(
            start(false, Some(keyframe(2_000, 67))).copy_blocker(),
            Some(CopyBlocker::StartsBetweenKeyframes)
        );
        assert_eq!(
            start(false, None).copy_blocker(),
            Some(CopyBlocker::NoKeyframes)
        );
    }

    #[test]
    fn missing_pts_uses_dts() {
        let stdout = "#tb 0: 1/1000\n0, 5000, -9223372036854775808, 33, 100, 0x0\n";
        let packets = parse_framecrc(stdout, Duration::ZERO);
        assert_eq!(packets[0].keyframe, keyframe(5000, 0));
    }

    #[test]
    fn two_stage_args_start_on_keyframe_dts() {
        let seek = CopySeek {
            start: Some(keyframe(20_020, 67)),
            end: None,
        };
        let args = seek.args(Duration::from_secs(30), FRAME);
        assert_eq!(args.input_seek, Duration::from_millis(18_952));
        assert_eq!(
            args.input_seek + args.output_seek.unwrap(),
            Duration::from_millis(20_020 - 67) - START_EPSILON
        );
        assert_eq!(args.duration, Duration::from_secs(30));
        assert_eq!(args.ts_offset_correction, Duration::from_millis(67));
    }

    #[test]
    fn chunk_ends_half_a_frame_before_end_keyframe_dts() {
        let seek = CopySeek {
            start: Some(keyframe(10_010, 67)),
            end: Some(keyframe(50_050, 67)),
        };
        let args = seek.args(Duration::from_secs(40), FRAME);
        let start_dts = Duration::from_millis(10_010 - 67) - START_EPSILON;
        assert_eq!(
            args.duration,
            Duration::from_millis(50_050 - 67) - FRAME / 2 - start_dts
        );
    }

    #[test]
    fn zero_start_chunk_has_no_seek() {
        let seek = CopySeek {
            start: None,
            end: Some(keyframe(40_040, 67)),
        };
        let args = seek.args(Duration::from_secs(40), FRAME);
        assert_eq!(args.input_seek, Duration::ZERO);
        assert_eq!(args.output_seek, None);
        assert_eq!(
            args.duration,
            Duration::from_millis(40_040 - 67) - FRAME / 2
        );
        assert_eq!(args.ts_offset_correction, Duration::ZERO);
    }

    #[test]
    fn early_keyframe_clamps_input_seek_to_zero() {
        let seek = CopySeek {
            start: Some(keyframe(500, 67)),
            end: None,
        };
        let args = seek.args(Duration::from_secs(10), FRAME);
        assert_eq!(args.input_seek, Duration::ZERO);
        assert_eq!(
            args.output_seek,
            Some(Duration::from_millis(500 - 67) - START_EPSILON)
        );
    }
}
