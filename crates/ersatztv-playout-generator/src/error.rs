use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlayoutGeneratorError {
    #[error("I/O Error: {0}")]
    IoFailure(#[from] std::io::Error),

    #[error("Indeterminate local time offset: {0}")]
    DateOffsetError(#[from] time::error::IndeterminateOffset),

    #[error("Date formatting error: {0}")]
    DateFormatError(#[from] time::error::Format),

    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("no source content was found")]
    NoSourceContent,

    #[error("unable to locate parent of lineup.json")]
    LineupNoParent,

    #[error("unable to locate channel in lineup.json")]
    LineupNoChannel,

    #[error("lineup error: {0}")]
    LineupError(#[from] ersatztv::error::LineupError),

    #[error("failed to load channel JSON file: {0}")]
    ChannelJsonLoadError(String),

    #[error("output folder is required")]
    NoOutputFolder,

    #[error("unable to locate parent of channel config {0}")]
    ChannelNoParent(String),

    #[error("error parsing date in xmltv: {0}")]
    FailedToParseXmltv(#[from] time::error::Parse),
}
