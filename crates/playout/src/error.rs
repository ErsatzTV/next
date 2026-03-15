use std::fmt::Formatter;

#[derive(Debug)]
pub enum PlayoutError {
    PlayoutJsonDoesNotExist,
    PlayoutJsonLoadError(String),
}

impl std::fmt::Display for PlayoutError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PlayoutError::PlayoutJsonDoesNotExist => write!(f, "playout json does not exist"),
            PlayoutError::PlayoutJsonLoadError(error) => {
                write!(f, "failed to load playout json file: {}", error)
            }
        }
    }
}

impl std::error::Error for PlayoutError {}
