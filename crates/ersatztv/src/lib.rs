use std::path::{Path, PathBuf};

use crate::error::LineupError;

pub mod config;
pub mod error;

pub fn validate_config_path(parent: &Path, config_path: &str) -> Result<PathBuf, LineupError> {
    let mut channel_config = PathBuf::from(config_path);
    if channel_config.is_relative() {
        let joined = parent.join(&channel_config);
        channel_config = match joined.canonicalize() {
            Ok(canonical) => canonical,
            _ => joined,
        };
    }

    if !channel_config.exists() {
        return Err(LineupError::ChannelConfigDoesNotExist(format!(
            "{:?}",
            channel_config
        )));
    }

    Ok(channel_config)
}
