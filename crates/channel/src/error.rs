use ffpipeline::error::FFPipelineError;
use std::fmt::Formatter;

pub enum ChannelError {
    PlayoutJsonUnsupported,
    PipelineError(FFPipelineError),
}

impl From<FFPipelineError> for ChannelError {
    fn from(value: FFPipelineError) -> Self {
        ChannelError::PipelineError(value)
    }
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelError::PlayoutJsonUnsupported => write!(
                f,
                "playout JSON is not yet supported; pass video file as arg"
            ),
            ChannelError::PipelineError(err) => write!(f, "{err}"),
        }
    }
}
