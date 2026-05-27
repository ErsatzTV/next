use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlayoutError {
    #[error("playout JSON does not exist")]
    PlayoutJsonDoesNotExist,

    #[error("failed to load playout JSON file: {0}")]
    PlayoutJsonLoadError(String),

    #[error("date formatting error: {0}")]
    PlayoutDateFormatError(#[from] time::error::Format),

    #[error("template references missing environment variable: {0}")]
    TemplateMissingEnvVar(String),

    #[error("environment variable '{0}' contains invalid characters (control chars not allowed)")]
    TemplateInvalidEnvVarValue(String),

    #[error("unrecognized schema version '{0}'")]
    UnrecognizedSchemaVersion(String),

    #[error("found unsupported schema version {0}, expected {1}")]
    UnsupportedSchemaVersion(String, String),
}
