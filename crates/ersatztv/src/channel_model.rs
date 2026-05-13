use std::path::{Path, PathBuf};

use crate::config::ChannelConfig;
use crate::error::LineupError;

pub struct ChannelModel {
    number: String,
    name: String,
    config_path: PathBuf,
    overlay_paths: Vec<PathBuf>,
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
        let parent = config_path
            .parent()
            .ok_or(LineupError::LineupConfigFailure(String::from(
                "failed to find parent of config",
            )))?;

        let channel_config = Self::validate_config_path(parent, &channel.config)?;

        let mut overlay_paths: Vec<PathBuf> = Vec::new();
        for overlay in channel.overlays {
            let overlay_path = Self::validate_config_path(parent, &overlay)?;
            overlay_paths.push(overlay_path);
        }

        let output_folder = PathBuf::from(output_folder);

        Ok(ChannelModel {
            number: channel.number.clone(),
            name: channel.name.clone(),
            config_path: channel_config,
            overlay_paths,
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

    pub fn overlay_paths(&self) -> &[PathBuf] {
        self.overlay_paths.as_ref()
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

    fn validate_config_path(parent: &Path, config_path: &str) -> Result<PathBuf, LineupError> {
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
}
