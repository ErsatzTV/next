use std::fmt::Formatter;

use crate::error::FFPipelineError;
use crate::probe::ProbeResult;

pub enum LogLevel {
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Error => write!(f, "error"),
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
            GlobalOption::Threads(count) => vec!["-threads".to_string(), count.to_string()],
            GlobalOption::NoStdIn => vec!["-nostdin".to_string()],
            GlobalOption::HideBanner => vec!["-hide_banner".to_string()],
            GlobalOption::LogLevel(level) => vec!["-loglevel".to_string(), level.to_string()],
        }
    }
}

pub enum PipelineInput {
    Video(String),
}

pub struct Pipeline {
    global_options: Vec<GlobalOption>,
    inputs: Vec<PipelineInput>,
}

impl Pipeline {
    fn full(probe_result: ProbeResult) -> Pipeline {
        Pipeline {
            global_options: vec![
                GlobalOption::Threads(0),
                GlobalOption::NoStdIn,
                GlobalOption::HideBanner,
                GlobalOption::LogLevel(LogLevel::Error),
            ],
            inputs: vec![PipelineInput::Video(probe_result.path)],
        }
    }

    pub fn args(&self) -> Vec<String> {
        let mut result: Vec<String> = Vec::new();

        result.extend(self.global_options.iter().flat_map(|o| o.as_arg()));

        for input in &self.inputs {
            match input {
                PipelineInput::Video(path) => result.extend(["-i".to_string(), path.clone()]),
            }
        }

        result
    }
}

impl std::fmt::Display for Pipeline {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "args: {}", self.args().join(" "))
    }
}

pub fn generate_pipeline(probe_result: ProbeResult) -> Result<Pipeline, FFPipelineError> {
    Ok(Pipeline::full(probe_result))
}
