use std::fmt::Formatter;
use std::process::Command;

use crate::error::FFPipelineError;

pub struct ProbeResult {
    json: String,
}

impl std::fmt::Display for ProbeResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.json)
    }
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

    String::from_utf8(output.stdout)
        .map_err(|_| FFPipelineError::ProbeFailed)
        .map(|s| ProbeResult { json: s })
}
