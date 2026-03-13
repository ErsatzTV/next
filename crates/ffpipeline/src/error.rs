use std::fmt::Formatter;

#[derive(Debug)]
pub enum FFPipelineError {
    ProbeFailed,
    ProbeFailedToParse,
}

impl std::fmt::Display for FFPipelineError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FFPipelineError::ProbeFailed => write!(f, "ffprobe failed"),
            FFPipelineError::ProbeFailedToParse => write!(f, "failed to parse ffprobe output"),
        }
    }
}

impl std::error::Error for FFPipelineError {}
