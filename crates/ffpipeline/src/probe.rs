use std::borrow::Cow;
use std::fmt::Formatter;
use std::path::Path;
use std::time::Duration;

use enum_dispatch::enum_dispatch;
use serde::Deserialize;
use tokio::process::Command;

use crate::ArgVec;
use crate::error::FFPipelineError;
use crate::error::FFPipelineError::ProbeFailed;
use crate::frame_rate::FrameRate;
use crate::input::InputSource;
use crate::input::LavfiInputSource;
use crate::input::LocalInputSource;
use crate::input::{FfmpegInputArgs, HttpInputSource};

#[derive(Debug, Clone)]
pub struct ProbeResultColorParams {
    pub color_range: Option<String>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
}

impl ProbeResultColorParams {
    pub fn is_hdr(&self) -> bool {
        self.color_transfer
            .as_ref()
            .is_some_and(|ct| ct == "arib-std-b67" || ct == "smpte2084")
    }
}

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
    pub color_params: ProbeResultColorParams,
    pub field_order: Option<String>,
}

impl ProbeResultVideoStream {
    pub fn is_interlaced(&self) -> bool {
        self.field_order
            .as_ref()
            .is_some_and(|fo| ["tt", "bb", "tb", "bt"].contains(&fo.as_str()))
    }

    pub fn is_anamorphic(&self) -> bool {
        // TODO: need to calculate SAR when it's not provided; port MediaStream::SampleAspectRatio

        match &self.sample_aspect_ratio {
            Some(sample_aspect_ratio) => {
                let display_aspect_ratio = self
                    .display_aspect_ratio
                    .as_ref()
                    .map_or("", |dar| dar.as_ref());

                // square pixels
                if sample_aspect_ratio == "1:1" {
                    false
                }
                // 0:1 is "unspecified", so anything other than that will be non-square/anamorphic
                else if sample_aspect_ratio != "0:1" {
                    true
                }
                // SAR 0:1 && DAR 0:1 (both unspecified) means square pixels
                else if display_aspect_ratio == "0:1" {
                    false
                } else {
                    // DAR == W:H is square
                    display_aspect_ratio != format!("{}:{}", self.width, self.height)
                }
            }
            None => false, // assumed SAR of 1:1
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProbeResultAudioStream {
    pub stream_index: u32,
    pub codec: String,
    pub channels: u32,
}

#[derive(Debug, Clone)]
pub enum ProbeResultStream {
    Video(Box<ProbeResultVideoStream>),
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
    color_range: Option<String>,
    color_space: Option<String>,
    color_transfer: Option<String>,
    color_primaries: Option<String>,
    field_order: Option<String>,
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

pub struct ProbeDeps<'a> {
    pub ffmpeg_path: &'a Path,
    pub ffprobe_path: &'a Path,
}

#[enum_dispatch]
pub trait Probeable {
    // Temporarily allow this - expanding out to the impl Future syntax
    // seems to break enum_dispatch.
    #[allow(async_fn_in_trait)]
    async fn probe(&self, probe_deps: &ProbeDeps<'_>) -> Result<ProbeResult, FFPipelineError>;
}

impl Probeable for LocalInputSource {
    async fn probe(&self, probe_deps: &ProbeDeps<'_>) -> Result<ProbeResult, FFPipelineError> {
        let mut args = args![
            "-hide_banner",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
        ];
        args.extend(self.args_for_input());
        let expanded_path = self.expand_path().ok_or(ProbeFailed)?;
        args.extend(args!["-i", expanded_path.clone()]);

        probe_with_args(probe_deps.ffprobe_path, &expanded_path, &args).await
    }
}

impl Probeable for LavfiInputSource {
    async fn probe(&self, probe_deps: &ProbeDeps<'_>) -> Result<ProbeResult, FFPipelineError> {
        let mut ffmpeg = Command::new(probe_deps.ffmpeg_path)
            .args([
                "-f",
                "lavfi",
                "-i",
                self.params.as_str(),
                "-t",
                "1",
                "-f",
                "nut",
                "pipe:1",
            ])
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

        let output = Command::new(probe_deps.ffprobe_path)
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

        parse_ffprobe_stdout(self.params.clone(), output.stdout)
    }
}

impl Probeable for HttpInputSource {
    async fn probe(&self, probe_deps: &ProbeDeps<'_>) -> Result<ProbeResult, FFPipelineError> {
        let mut args: ArgVec = args![
            "-hide_banner",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
        ];
        args.extend(self.args_for_input());
        args.extend(args!["-i", self.uri.clone()]);

        probe_with_args(probe_deps.ffprobe_path, &self.uri, &args).await
    }
}

async fn probe_with_args(
    ffprobe_path: &Path,
    path: &str,
    args: &ArgVec,
) -> Result<ProbeResult, FFPipelineError> {
    let output = Command::new(ffprobe_path)
        .args(args.iter().map(Cow::as_ref))
        .output()
        .await
        .map_err(|_| FFPipelineError::ProbeFailed)?;

    if !output.status.success() {
        log::warn!(
            "error executing ffprobe: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(FFPipelineError::ProbeFailed);
    }

    parse_ffprobe_stdout(path.to_owned(), output.stdout)
}

fn parse_ffprobe_stdout(path: String, stdout: Vec<u8>) -> Result<ProbeResult, FFPipelineError> {
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
        "video" => Some(ProbeResultStream::Video(Box::new(ProbeResultVideoStream {
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
            color_params: ProbeResultColorParams {
                color_range: output_stream.color_range.clone(),
                color_space: output_stream.color_space.clone(),
                color_transfer: output_stream.color_transfer.clone(),
                color_primaries: output_stream.color_primaries.clone(),
            },
            field_order: output_stream.field_order.clone(),
            frame_rate: FrameRate::parse(&output_stream.r_frame_rate.clone()?),
            sample_aspect_ratio: output_stream.sample_aspect_ratio.to_owned(),
            display_aspect_ratio: output_stream.display_aspect_ratio.to_owned(),
        }))),
        _ => None,
    }
}
