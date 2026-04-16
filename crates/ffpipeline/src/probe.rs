use std::fmt::Formatter;
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;

use crate::error::FFPipelineError;
use crate::frame_rate::FrameRate;

#[derive(Debug, Clone)]
pub struct ProbeResultVideoStream {
    pub stream_index: u32,
    pub codec: String,
    pub profile: String,
    pub height: u32,
    pub width: u32,
    pub frame_rate: FrameRate,
    pub sample_aspect_ratio: Option<String>,
    pub display_aspect_ratio: Option<String>,
    pub pix_fmt: String,
}

#[derive(Debug, Clone)]
pub struct ProbeResultAudioStream {
    pub stream_index: u32,
    pub codec: String,
    pub channels: u32,
}

#[derive(Debug, Clone)]
pub enum ProbeResultStream {
    Video(ProbeResultVideoStream),
    Audio(ProbeResultAudioStream),
}

impl std::fmt::Display for ProbeResultStream {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeResultStream::Audio(a) => {
                write!(
                    f,
                    "{}: audio ({} - {} channels)",
                    a.stream_index, a.codec, a.channels
                )
            }
            ProbeResultStream::Video(v) => {
                write!(
                    f,
                    "{}: video ({} - {}x{} - {:?})",
                    v.stream_index, v.codec, v.width, v.height, v.frame_rate
                )
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub path: String,
    pub streams: Vec<ProbeResultStream>,
    pub duration: Option<Duration>,
}

impl std::fmt::Display for ProbeResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(duration) = self.duration {
            writeln!(f, "duration: {}s", duration.as_secs_f64())?;
        }

        write!(
            f,
            "{}",
            &self
                .streams
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
                .join("\n")
        )
    }
}

impl ProbeResult {
    pub fn is_still_image(&self) -> bool {
        // TODO: better check
        self.duration.is_none() && self.streams.len() == 1
    }
}

#[derive(Deserialize)]
struct ProbeOutputStream {
    index: u32,
    codec_type: String,
    codec_name: Option<String>,
    profile: Option<String>,
    height: Option<u32>,
    width: Option<u32>,
    channels: Option<u32>,
    r_frame_rate: Option<String>,
    sample_aspect_ratio: Option<String>,
    display_aspect_ratio: Option<String>,
    pix_fmt: Option<String>,
}

#[derive(Deserialize)]
struct ProbeOutputFormat {
    duration: Option<String>,
}

#[derive(Deserialize)]
struct ProbeOutput {
    streams: Vec<ProbeOutputStream>,
    format: ProbeOutputFormat,
}

pub async fn probe(
    ffprobe_path: &std::path::Path,
    path: &str,
) -> Result<ProbeResult, FFPipelineError> {
    let output = Command::new(ffprobe_path)
        .args([
            "-hide_banner",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
            "-i",
            path,
        ])
        .output()
        .await
        .map_err(|_| FFPipelineError::ProbeFailed)?;

    if !output.status.success() {
        return Err(FFPipelineError::ProbeFailed);
    }

    parse_ffprobe_stdout(path, output.stdout)
}

pub async fn probe_lavfi(
    ffprobe_path: &std::path::Path,
    ffmpeg_path: &std::path::Path,
    lavfi: &str,
) -> Result<ProbeResult, FFPipelineError> {
    let mut ffmpeg = Command::new(ffmpeg_path)
        .args(["-f", "lavfi", "-i", lavfi, "-t", "1", "-f", "nut", "pipe:1"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| FFPipelineError::ProbeFailed)?;

    let ffmpeg_stdout: std::process::Stdio = ffmpeg
        .stdout
        .take()
        .ok_or(FFPipelineError::ProbeFailed)?
        .try_into()
        .map_err(|_| FFPipelineError::ProbeFailed)?;

    let output = Command::new(ffprobe_path)
        .args([
            "-hide_banner",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
            "-i",
            "pipe:0",
        ])
        .stdin(ffmpeg_stdout)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .map_err(|_| FFPipelineError::ProbeFailed)?;

    let _ = ffmpeg.wait().await;

    if !output.status.success() {
        return Err(FFPipelineError::ProbeFailed);
    }

    parse_ffprobe_stdout(lavfi, output.stdout)
}

fn parse_ffprobe_stdout(path: &str, stdout: Vec<u8>) -> Result<ProbeResult, FFPipelineError> {
    let raw_output = String::from_utf8(stdout).map_err(|_| FFPipelineError::ProbeFailedToParse)?;

    //println!("{raw_output}");

    let deserialized = serde_json::from_str::<ProbeOutput>(&raw_output);

    match deserialized {
        Err(err) => {
            log::error!("{err}");
            Err(FFPipelineError::ProbeFailedToParse)
        }
        Ok(output) => {
            let streams: Vec<ProbeResultStream> =
                output.streams.iter().flat_map(output_to_result).collect();

            let duration = output
                .format
                .duration
                .and_then(|s| s.parse::<f64>().ok())
                .map(Duration::from_secs_f64);

            Ok(ProbeResult {
                path: path.to_owned(),
                streams,
                duration,
            })
        }
    }
}

fn output_to_result(output_stream: &ProbeOutputStream) -> Option<ProbeResultStream> {
    match output_stream.codec_type.to_lowercase().as_str() {
        "audio" => Some(ProbeResultStream::Audio(ProbeResultAudioStream {
            stream_index: output_stream.index,
            codec: output_stream
                .codec_name
                .clone()
                .unwrap_or(String::from("unknown")),
            channels: output_stream.channels?,
        })),
        "video" => Some(ProbeResultStream::Video(ProbeResultVideoStream {
            stream_index: output_stream.index,
            codec: output_stream
                .codec_name
                .clone()
                .map_or(String::from("unknown"), |c| c.to_lowercase()),
            profile: output_stream
                .profile
                .clone()
                .map_or(String::new(), |p| p.to_lowercase()),
            height: output_stream.height?,
            width: output_stream.width?,
            pix_fmt: output_stream.pix_fmt.clone()?,
            frame_rate: FrameRate::parse(&output_stream.r_frame_rate.clone()?),
            sample_aspect_ratio: output_stream.sample_aspect_ratio.to_owned(),
            display_aspect_ratio: output_stream.display_aspect_ratio.to_owned(),
        })),
        _ => None,
    }
}
