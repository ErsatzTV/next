use std::fmt::Formatter;

use crate::error::FFPipelineError;
use crate::probe::ProbeResult;

pub enum LogLevel {
    Error,
}

impl LogLevel {
    fn as_arg(&self) -> String {
        match self {
            LogLevel::Error => String::from("error"),
        }
    }
}

pub enum GlobalOption {
    Threads(u32),
    NoStdIn,
    HideBanner,
    LogLevel(LogLevel),
}

impl GlobalOption {
    fn as_arg(&self) -> Vec<String> {
        match self {
            GlobalOption::Threads(count) => vec![String::from("-threads"), count.to_string()],
            GlobalOption::NoStdIn => vec![String::from("-nostdin")],
            GlobalOption::HideBanner => vec![String::from("-hide_banner")],
            GlobalOption::LogLevel(level) => vec![String::from("-loglevel"), level.as_arg()],
        }
    }
}

pub enum OutputFormat {
    Hls,
}

impl OutputFormat {
    fn as_arg(&self) -> Vec<String> {
        match self {
            OutputFormat::Hls => [
                "-f",
                "hls",
                "-hls_time",
                "4",
                "-hls_list_size",
                "0",
                "-segment_list_flags",
                "+live",
                "-hls_segment_type",
                "mpegts",
            ]
            .map(String::from)
            .to_vec(),
        }
    }
}

pub enum AudioCodec {
    Copy,
}

impl AudioCodec {
    fn as_arg(&self) -> Vec<String> {
        match self {
            AudioCodec::Copy => vec![String::from("-acodec"), String::from("copy")],
        }
    }
}

pub enum VideoCodec {
    Copy,
}

impl VideoCodec {
    fn as_arg(&self) -> Vec<String> {
        match self {
            VideoCodec::Copy => vec![String::from("-vcodec"), String::from("copy")],
        }
    }
}

pub enum OutputOption {
    Format(OutputFormat),
    VideoCodec(VideoCodec),
    AudioCodec(AudioCodec),
    Duration(std::time::Duration),
}

impl OutputOption {
    fn as_arg(&self) -> Vec<String> {
        match self {
            OutputOption::Format(format) => format.as_arg(),
            OutputOption::VideoCodec(codec) => codec.as_arg(),
            OutputOption::AudioCodec(codec) => codec.as_arg(),
            OutputOption::Duration(duration) => {
                vec![String::from("-t"), format!("{}s", duration.as_secs_f64())]
            }
        }
    }
}

pub enum PipelineInput {
    Video(String),
}

pub struct PipelineOutput {
    path: String,
}

pub struct Pipeline {
    global_options: Vec<GlobalOption>,
    inputs: Vec<PipelineInput>,
    output_options: Vec<OutputOption>,
    output: PipelineOutput,
}

impl Pipeline {
    fn full(probe_result: ProbeResult, output: String) -> Pipeline {
        let duration = match probe_result.duration {
            Some(probed_duration) => probed_duration.min(std::time::Duration::from_secs(30)),
            None => std::time::Duration::from_secs(30),
        };

        Pipeline {
            global_options: vec![
                GlobalOption::Threads(0),
                GlobalOption::NoStdIn,
                GlobalOption::HideBanner,
                GlobalOption::LogLevel(LogLevel::Error),
            ],
            inputs: vec![PipelineInput::Video(probe_result.path)],
            output_options: vec![
                OutputOption::AudioCodec(AudioCodec::Copy),
                OutputOption::VideoCodec(VideoCodec::Copy),
                OutputOption::Format(OutputFormat::Hls),
                OutputOption::Duration(duration),
            ],
            output: PipelineOutput { path: output },
        }
    }

    pub fn args(&self) -> Vec<String> {
        let mut result: Vec<String> = Vec::new();

        result.extend(self.global_options.iter().flat_map(|o| o.as_arg()));

        for input in &self.inputs {
            match input {
                PipelineInput::Video(path) => result.extend([String::from("-i"), path.to_owned()]),
            }
        }

        // TODO: filter_complex

        result.extend(self.output_options.iter().flat_map(|o| o.as_arg()));

        result.extend([self.output.path.to_owned()]);

        result
    }
}

impl std::fmt::Display for Pipeline {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "args: {}", self.args().join(" "))
    }
}

pub fn generate_pipeline(
    probe_result: ProbeResult,
    output: String,
) -> Result<Pipeline, FFPipelineError> {
    Ok(Pipeline::full(probe_result, output))
}
