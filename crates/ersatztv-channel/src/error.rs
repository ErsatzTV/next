use std::path::Path;

use ersatztv_playout::error::PlayoutError;
use ffpipeline::error::FFPipelineError;
use thiserror::Error;
use time::OffsetDateTime;

#[derive(Error, Debug)]
pub enum ChannelError {
    #[error("unable to load channel config: {0}")]
    ChannelConfigFailure(String),

    /// An io failure, named by what was being done and to what.
    ///
    /// Deliberately has no `#[from]`: an io error carries neither the
    /// operation nor the subject, and a bare ENOENT is the same value
    /// whether a segment was trimmed, a temp file renamed, or a directory
    /// scanned. Without the conversion a bare `?` on an io result is a
    /// compile error, so a new call site cannot join an anonymous pool by
    /// accident.
    #[error("failed to {operation} {subject}: {source}")]
    Io {
        operation: &'static str,
        subject: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to expand output folder {0}")]
    ChannelConfigExpandOutputFolder(String),

    #[error("output path for {file} is not valid UTF-8: {path}")]
    OutputPathNotUtf8 { file: &'static str, path: String },

    #[error("playout folder contains a non-UTF-8 path: {0}")]
    PlayoutPathNotUtf8(String),

    #[error("channel startup error: {0}")]
    ChannelStartup(String),

    #[error("date formatting error: {0}")]
    ChannelDateFormatError(#[from] time::error::Format),

    #[error("Indeterminate local time offset: {0}")]
    DateOffsetError(#[from] time::error::IndeterminateOffset),

    #[error("{0}")]
    PlayoutJsonLoadFailure(#[from] PlayoutError),

    #[error("unable to find playout JSON file for time {0}")]
    PlayoutJsonNoFileForTime(OffsetDateTime),

    #[error("unable to find current item in playout JSON")]
    PlayoutJsonNoItem { next_start: Option<OffsetDateTime> },

    // This value got pushed down into another module (pipeline)
    // See if there is a way to port this over
    // #[error("local source is invalid for playout item")]
    // PlayoutJsonInvalidLocalSource,
    #[error("audio source is required for playout item")]
    PlayoutJsonAudioSourceRequired,

    #[error("video source is required for playout item")]
    PlayoutJsonVideoSourceRequired,

    #[error("{0}")]
    PipelineError(#[from] FFPipelineError),

    #[error("stream failed: {0}")]
    StreamFailure(String),

    #[error("last segment path is not valid UTF-8: {0}")]
    PtsScannerPathNotUtf8(String),

    #[error("channel {0} terminated after idle timeout")]
    IdleTimeout(String),

    #[error("channel {0} terminated after ffmpeg stall")]
    Stalled(String),

    #[error("failed to capture ffmpeg stderr")]
    CaptureFFmpegStderrFailure,

    #[error("dynamic source is required")]
    DynamicSourceRequired,

    #[error("dynamic source cannot be played directly")]
    DynamicSourceCannotBePlayedDirectly,

    #[error("dynamic source failure: {0}")]
    DynamicSourceFailure(String),

    #[error("dynamic source has no remaining time in window")]
    DynamicSourceNoRemainingTime,

    #[error("dynamic sources cannot return dynamic sources")]
    DynamicSourceCannotRecurse,

    #[error("probe hint failure")]
    ProbeHintFailure,
}

/// Names an io failure at the point the call is made, because neither the
/// operation nor its subject can be recovered from the error afterwards.
///
/// `operation` completes "failed to ...", so it reads as a verb phrase that
/// ends where the subject begins ("delete the trimmed segment"). It is
/// `&'static str` so it cannot absorb variable data: it stays a stable grep
/// key while the subject varies.
///
/// Where a value is written through a temp file, the subject is the
/// DESTINATION, never `temp.path()`. A temp name is random, is unlinked
/// before anyone reads the log, and identifies nothing; the operation says
/// which phase failed.
pub trait IoContext<T> {
    /// For a subject that is a path.
    fn io_context(
        self,
        operation: &'static str,
        subject: impl AsRef<Path>,
    ) -> Result<T, ChannelError>;

    /// For the few subjects that are not paths (stdin, a socket address).
    fn io_context_named(self, operation: &'static str, subject: &str) -> Result<T, ChannelError>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn io_context(
        self,
        operation: &'static str,
        subject: impl AsRef<Path>,
    ) -> Result<T, ChannelError> {
        self.map_err(|source| ChannelError::Io {
            operation,
            subject: subject.as_ref().display().to_string(),
            source,
        })
    }

    fn io_context_named(self, operation: &'static str, subject: &str) -> Result<T, ChannelError> {
        self.map_err(|source| ChannelError::Io {
            operation,
            subject: subject.to_owned(),
            source,
        })
    }
}
