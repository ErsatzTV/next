use std::fmt::Formatter;
use std::process::Command;

use crate::error::FFPipelineError;

use serde::Deserialize;

pub enum ProbeResultStreamType {
    Audio,
    Video,
}

impl std::fmt::Display for ProbeResultStreamType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeResultStreamType::Audio => write!(f, "audio"),
            ProbeResultStreamType::Video => write!(f, "video"),
        }
    }
}

pub struct ProbeResultStream {
    stream_index: u32,
    stream_type: ProbeResultStreamType,
    stream_codec: String,
}

impl std::fmt::Display for ProbeResultStream {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.stream_type {
            ProbeResultStreamType::Audio => {
                write!(
                    f,
                    "{}: {} ({})",
                    self.stream_index, self.stream_type, self.stream_codec
                )
            }
            ProbeResultStreamType::Video => {
                write!(
                    f,
                    "{}: {} ({})",
                    self.stream_index, self.stream_type, self.stream_codec
                )
            }
        }
    }
}

pub struct ProbeResult {
    streams: Vec<ProbeResultStream>,
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
        "audio" => Some(ProbeResultStream {
            stream_index: output_stream.index,
            stream_type: ProbeResultStreamType::Audio,
            stream_codec: output_stream.codec_name.clone(),
        }),
        "video" => Some(ProbeResultStream {
            stream_index: output_stream.index,
            stream_type: ProbeResultStreamType::Video,
            stream_codec: output_stream.codec_name.clone(),
        }),
        _ => None,
    }
}
