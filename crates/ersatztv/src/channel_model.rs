use std::path::{Path, PathBuf};

use crate::config::ChannelConfig;
use crate::error::LineupError;

pub struct ChannelModel {
    number: String,
    name: String,
    config_path: PathBuf,
    output_folder: PathBuf,
    tvg_id: String,
    logo: Option<String>,
    group: Option<String>,
}

impl ChannelModel {
    pub fn new(
        config_path: &Path,
        output_folder: &str,
        channel: ChannelConfig,
    ) -> Result<ChannelModel, LineupError> {
        let mut channel_config = PathBuf::from(&channel.config);
        if channel_config.is_relative() {
            let parent = config_path
                .parent()
                .ok_or(LineupError::LineupConfigFailure(String::from(
                    "failed to find parent of config",
                )))?;
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

        let output_folder = PathBuf::from(output_folder);

        Ok(ChannelModel {
            number: channel.number.clone(),
            name: channel.name.clone(),
            config_path: channel_config,
            output_folder: output_folder.join(&channel.number),
            tvg_id: channel.tvg_id.unwrap_or_else(|| channel.number.clone()),
            logo: channel.logo.clone(),
            group: channel.group.clone(),
        })
    }

    pub fn number(&self) -> &str {
        self.number.as_str()
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn config_path(&self) -> &Path {
        self.config_path.as_path()
    }

    pub fn output_folder(&self) -> &Path {
        self.output_folder.as_path()
    }

    pub fn tvg_id(&self) -> &str {
        self.tvg_id.as_str()
    }

    pub fn logo(&self) -> Option<&str> {
        self.logo.as_deref()
    }

    pub fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }
}
