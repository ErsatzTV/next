use std::fmt::Formatter;

#[derive(Debug)]
pub enum FFPipelineError {
    ProbeFailed,
}

impl std::fmt::Display for FFPipelineError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FFPipelineError::ProbeFailed => write!(f, "ffprobe failed"),
        }
    }
}

impl std::error::Error for FFPipelineError {}
