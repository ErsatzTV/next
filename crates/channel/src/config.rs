use serde::Deserialize;

use crate::error::ChannelError;

#[derive(Deserialize)]
pub struct ChannelConfig {
    pub playout: PlayoutConfig,
}

#[derive(Deserialize)]
pub struct PlayoutConfig {
    pub folder: String,
}

pub fn from_file(path: &str) -> Result<ChannelConfig, ChannelError> {
    let config_string = std::fs::read_to_string(path)
        .map_err(|e| ChannelError::ChannelConfigFailure(e.to_string()))?;
    let channel_config: ChannelConfig = toml::from_str(&config_string)
        .map_err(|e| ChannelError::ChannelConfigFailure(e.to_string()))?;
    Ok(channel_config)
}
