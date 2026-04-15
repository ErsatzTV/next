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

    #[error("sqlite error: {0}")]
    SqliteError(#[from] sqlx::Error),

    #[error("no source content was found")]
    NoSourceContent,
}
