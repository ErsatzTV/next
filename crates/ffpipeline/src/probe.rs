use std::fmt::Formatter;
use std::process::Command;

use serde::Deserialize;

use crate::error::FFPipelineError;

pub struct ProbeResultVideoStream {
    pub stream_index: u32,
    pub codec: String,
    pub height: u32,
    pub width: u32,
}

pub struct ProbeResultAudioStream {
    pub stream_index: u32,
    pub codec: String,
    pub channels: u32,
}

pub enum ProbeResultStream {
    Video(ProbeResultVideoStream),
    Audio(ProbeResultAudioStream)
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
                    "{}: video ({} - {}x{})",
                    v.stream_index, v.codec, v.width, v.height
                )
            }
        }
    }
}

pub struct ProbeResult {
    pub streams: Vec<ProbeResultStream>,
}

impl std::fmt::Display for ProbeResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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

#[derive(Deserialize)]
struct ProbeOutputStream {
    index: u32,
    codec_type: String,
    codec_name: String,
    height: Option<u32>,
    width: Option<u32>,
    channels: Option<u32>,
}

#[derive(Deserialize)]
struct ProbeOutput {
    streams: Vec<ProbeOutputStream>,
}

pub fn probe(path: &str) -> Result<ProbeResult, FFPipelineError> {
    let output = Command::new("ffprobe")
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
        .map_err(|_| FFPipelineError::ProbeFailed)?;

    if !output.status.success() {
        return Err(FFPipelineError::ProbeFailed);
    }

    let raw_output =
        String::from_utf8(output.stdout).map_err(|_| FFPipelineError::ProbeFailedToParse)?;

    //println!("{raw_output}");

    let deserialized = serde_json::from_str::<ProbeOutput>(raw_output.as_str());

    match deserialized {
        Err(err) => {
            eprintln!("{err}");
            Err(FFPipelineError::ProbeFailedToParse)
        }
        Ok(output) => {
            let streams: Vec<ProbeResultStream> =
                output.streams.iter().flat_map(output_to_result).collect();
            Ok(ProbeResult { streams })
        }
    }
}

fn output_to_result(output_stream: &ProbeOutputStream) -> Option<ProbeResultStream> {
    match output_stream.codec_type.to_lowercase().as_str() {
        "audio" => Some(ProbeResultStream::Audio(ProbeResultAudioStream {
            stream_index: output_stream.index,
            codec: output_stream.codec_name.clone(),
            channels: output_stream.channels?,
        })),
        "video" => Some(ProbeResultStream::Video(ProbeResultVideoStream {
            stream_index: output_stream.index,
            codec: output_stream.codec_name.clone(),
            height: output_stream.height?,
            width: output_stream.width?,
        })),
        _ => None,
    }
}
