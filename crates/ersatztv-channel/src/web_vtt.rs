use std::path::{Path, PathBuf};
use std::time::Duration;

use ersatztv_channel::error::ChannelError;
use ffpipeline::input::{InputSource, ProbedInput};
use ffpipeline::probe::{CodecType, ProbeResultStream, ProbeResultVideoStream};
use tempfile::NamedTempFile;
use tokio::process::Command;

#[derive(Clone)]
pub struct Cue {
    pub start: Duration,
    pub end: Duration,
    pub text: String,
}

pub(crate) async fn convert_to_vtt(
    ffmpeg_path: &PathBuf,
    input: &ProbedInput,
    subtitle_stream: &ProbeResultVideoStream,
) -> Result<NamedTempFile, ChannelError> {
    match &input.input_source {
        InputSource::Local(local) => {
            // find index of subtitle *within subtitle streams*
            let subtitle_index = input
                .probe_result
                .streams
                .iter()
                .filter_map(|s| match s {
                    ProbeResultStream::Video(v) if v.codec_type == CodecType::Subtitle => Some(v),
                    _ => None,
                })
                .position(|v| v.stream_index == subtitle_stream.stream_index)
                .ok_or(ChannelError::FailedToConvertSubtitle)?;

            let temp_file = NamedTempFile::with_suffix(".vtt")?;
            let file_name = temp_file.path().to_string_lossy();
            let mut ffmpeg = Command::new(ffmpeg_path)
                .args([
                    "-nostdin",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-i",
                    local.path.as_str(),
                    "-map",
                    &format!("0:s:{}", subtitle_index),
                    "-c:s",
                    "webvtt",
                    "-y",
                    &file_name,
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|_| ChannelError::FailedToConvertSubtitle)?;

            let result = ffmpeg.wait().await?;
            if result.success() {
                Ok(temp_file)
            } else {
                Err(ChannelError::FailedToConvertSubtitle)
            }
        }
        _ => Err(ChannelError::FailedToConvertSubtitle),
    }
}

pub(crate) async fn parse_file(path: &Path) -> Result<Vec<Cue>, ChannelError> {
    if path.exists() {
        let contents = tokio::fs::read_to_string(&path).await?;
        return parse_internal(&contents);
    }

    Err(ChannelError::FailedToParseSubtitle)
}

pub(crate) fn format_vtt_ts(duration: Duration) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        duration.as_secs() / 3600,
        (duration.as_secs() / 60) % 60,
        duration.as_secs() % 60,
        duration.subsec_millis()
    )
}

#[derive(PartialEq)]
enum ParseState {
    Header,
    Body,
    Cue,
}

fn parse_internal(body: &str) -> Result<Vec<Cue>, ChannelError> {
    let mut result = Vec::new();
    let mut parse_state = ParseState::Header;
    let mut start = Duration::ZERO;
    let mut end = Duration::ZERO;
    let mut text = String::new();

    for line in body.lines() {
        match parse_state {
            ParseState::Header => {
                if line.starts_with("WEBVTT") {
                    parse_state = ParseState::Body;
                }
            }
            ParseState::Body => {
                if line.contains(" --> ") {
                    let split: Vec<&str> = line.split(" --> ").collect();
                    if split.len() == 2
                        && let Some(parsed_start) = parse_timestamp(split[0])
                        && let Some(parsed_end) = parse_timestamp(split[1])
                    {
                        parse_state = ParseState::Cue;
                        start = parsed_start;
                        end = parsed_end;
                    }
                }
            }
            ParseState::Cue => {
                if !line.is_empty() {
                    if !line.starts_with("NOTE") && !line.starts_with("STYLE") {
                        text.push_str(line);
                        text.push('\n');
                    }
                } else {
                    result.push(Cue {
                        start,
                        end,
                        text: text.trim().to_string(),
                    });
                    text.clear();
                    parse_state = ParseState::Body;
                }
            }
        }
    }

    if parse_state == ParseState::Cue {
        result.push(Cue {
            start,
            end,
            text: text.trim().to_string(),
        });
    }

    Ok(result)
}

fn parse_timestamp(s: &str) -> Option<Duration> {
    let mut parts: Vec<&str> = s.split(':').collect();

    // check for missing hours
    if parts.len() == 2 {
        parts.insert(0, "0");
    }

    if parts.len() != 3 {
        return None;
    }

    let hours: u64 = parts[0].parse().ok()?;
    let minutes: u64 = parts[1].parse().ok()?;
    let seconds_f: f64 = parts[2].parse().ok()?;
    Some(Duration::from_secs(hours * 3600 + minutes * 60) + Duration::from_secs_f64(seconds_f))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn vtt_should_parse() {
        let vtt_text = r"WEBVTT
00:00:00.500 --> 00:00:02.000
The Web is always changing

00:00:02.500 --> 00:00:04.300
and the way we access it is changing
";

        let result = parse_internal(vtt_text);
        let parsed = result.unwrap_or_default();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].start, Duration::from_secs_f64(0.5));
        assert_eq!(parsed[0].end, Duration::from_secs(2));
        assert_eq!(parsed[0].text, "The Web is always changing");
        assert_eq!(parsed[1].start, Duration::from_secs_f64(2.5));
        assert_eq!(parsed[1].end, Duration::from_secs_f64(4.3));
        assert_eq!(parsed[1].text, "and the way we access it is changing");
    }

    #[test]
    fn vtt_should_parse_mm_ss() {
        let vtt_text = r"WEBVTT

00:02.499 --> 00:06.416
[SERENE MUSIC]

00:11.791 --> 00:13.958
[BROOK BABBLES]
[FLY BUZZES]

00:16.166 --> 00:17.666
[BIRD TWEETS]
";

        let result = parse_internal(vtt_text);
        let parsed = result.unwrap_or_default();

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].start, Duration::from_secs_f64(2.499));
        assert_eq!(parsed[0].end, Duration::from_secs_f64(6.416));
        assert_eq!(parsed[0].text, "[SERENE MUSIC]");
        assert_eq!(parsed[1].start, Duration::from_secs_f64(11.791));
        assert_eq!(parsed[1].end, Duration::from_secs_f64(13.958));
        assert_eq!(parsed[1].text, "[BROOK BABBLES]\n[FLY BUZZES]");
        assert_eq!(parsed[2].start, Duration::from_secs_f64(16.166));
        assert_eq!(parsed[2].end, Duration::from_secs_f64(17.666));
        assert_eq!(parsed[2].text, "[BIRD TWEETS]");
    }

    #[rstest]
    #[case("00:00:01.5", 1.5)]
    #[case("00:01.5", 1.5)]
    #[case("00:00:01.005", 1.005)]
    #[case("00:01.005", 1.005)]
    fn parse_timestamp_seconds_float(#[case] input: &str, #[case] expected: f64) {
        let duration = parse_timestamp(input);
        assert_eq!(duration, Some(Duration::from_secs_f64(expected)));
    }
}
