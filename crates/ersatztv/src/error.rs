use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LineupError {
    #[error("io error {0}")]
    Io(#[from] std::io::Error),

    #[error("unable to load lineup config: {0}")]
    LineupConfigFailure(String),

    #[error("channel config does not exist at path: {0}")]
    ChannelConfigDoesNotExist(String),

    #[error("no channels have been loaded; please review your lineup config")]
    NoChannelsLoaded,

    #[error("unable to find channel with number {0}")]
    ChannelNotFound(String),

    #[error("channel is not yet ready")]
    ChannelNotReady,

    #[error("unable to locate parent of lineup.json")]
    ScaffoldNoParent,

    #[error("the following paths already exist: {0}")]
    ScaffoldPathsExist(String),

    #[error("failed to serialize JSON")]
    ScaffoldSerializeError(#[from] serde_json::Error),

    #[error("channel already exists")]
    ScaffoldChannelExists,
}

impl IntoResponse for LineupError {
    fn into_response(self) -> Response {
        match self {
            LineupError::ChannelNotFound(_) => {
                (StatusCode::NOT_FOUND, self.to_string()).into_response()
            }
            LineupError::ChannelNotReady => (
                StatusCode::SERVICE_UNAVAILABLE,
                [(axum::http::header::RETRY_AFTER, "5")],
                "channel not ready",
            )
                .into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response(),
        }
    }
}
