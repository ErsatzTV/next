use std::fmt::Formatter;

use ersatztv_playout::error::PlayoutError;
use ffpipeline::error::FFPipelineError;

pub enum ChannelError {
    ChannelConfigRequired,
    ChannelConfigFailure(String),
    PlayoutJsonLoadFailure(PlayoutError),
    PlayoutJsonNoItem,
    PlayoutJsonSingleSourceRequired,
    PlayoutJsonLocalSourceRequired,
    PipelineError(FFPipelineError),
}

impl From<PlayoutError> for ChannelError {
    fn from(value: PlayoutError) -> Self {
        ChannelError::PlayoutJsonLoadFailure(value)
    }
}

impl From<FFPipelineError> for ChannelError {
    fn from(value: FFPipelineError) -> Self {
        ChannelError::PipelineError(value)
    }
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelError::ChannelConfigRequired => write!(f, "channel config is required as arg"),
            ChannelError::ChannelConfigFailure(err) => {
                write!(f, "unable to load channel config: {err}")
            }
            ChannelError::PlayoutJsonLoadFailure(err) => write!(f, "{err}"),
            ChannelError::PlayoutJsonNoItem => {
                write!(f, "unable to find current item in playout JSON")
            }
            ChannelError::PlayoutJsonSingleSourceRequired => {
                write!(f, "only single sources are supported as playout items")
            }
            ChannelError::PlayoutJsonLocalSourceRequired => {
                write!(f, "only local sources are supported as playout items")
            }
            ChannelError::PipelineError(err) => write!(f, "{err}"),
        }
    }
}
