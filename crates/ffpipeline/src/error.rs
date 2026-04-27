use thiserror::Error;

#[derive(Error, Debug)]
pub enum FFPipelineError {
    #[error("error detecting ffmpeg capabilities: {0}")]
    FfmpegCapabilitiesError(String),
    #[error("ffprobe failed")]
    ProbeFailed,
    #[error("failed to parse ffprobe output")]
    ProbeFailedToParse,
    #[error("audio input is required")]
    AudioInputIsRequired,
    #[error("video input is required")]
    VideoInputIsRequired,
    #[error("error detecting nvidia capabilities: {0}")]
    NvidiaCapabilitiesError(String),
    #[error("error detecting opencl capabilities: {0}")]
    OpenCLCapabilitiesError(String),
    #[error("error detecting qsv capabilities: {0}")]
    QsvCapabilitiesError(String),
    #[error("error detecting vaapi capabilities: {0}")]
    VaapiCapabilitiesError(String),
    #[error("error detecting videotoolbox capabilities: {0}")]
    VideoToolboxCapabilitiesError(String),
}
