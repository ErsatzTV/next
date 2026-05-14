use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simple_expand_tilde::expand_tilde;

use crate::error::LineupError;

#[derive(Deserialize, Serialize, Clone, JsonSchema)]
pub struct LineupConfig {
    #[serde(default = "server_config_default")]
    pub server: ServerConfig,
    pub output: OutputConfig,
    pub channels: Vec<ChannelConfig>,
}

#[derive(Deserialize, Serialize, Clone, JsonSchema)]
pub struct ServerConfig {
    #[serde(default = "bind_address_default")]
    pub bind_address: String,
    #[serde(default = "port_default")]
    pub port: u16,
}

#[derive(Deserialize, Serialize, Clone, JsonSchema)]
pub struct OutputConfig {
    pub folder: String,
}

#[derive(Deserialize, Serialize, Clone, JsonSchema)]
pub struct ChannelConfig {
    pub number: String,
    pub name: String,
    /// Base configuration path
    pub config: String,
    /// Optional configuration overlay paths; values will be merged with base config, nulls will remove keys from base config
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlays: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tvg_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

impl ChannelConfig {
    pub fn scaffold(number: &str) -> Self {
        Self {
            number: number.to_string(),
            name: format!("Channel {number}"),
            config: format!("./channels/{number}/channel.json"),
            overlays: Vec::new(),
            group: None,
            logo: None,
            tvg_id: None,
        }
    }
}

fn server_config_default() -> ServerConfig {
    ServerConfig {
        bind_address: bind_address_default(),
        port: port_default(),
    }
}

fn bind_address_default() -> String {
    String::from("0.0.0.0")
}
fn port_default() -> u16 {
    8409
}

pub async fn from_file(path: &PathBuf) -> Result<LineupConfig, LineupError> {
    if !path.exists() {
        return Err(LineupError::LineupConfigFailure(format!(
            "file does not exist: {:?}",
            path
        )));
    }

    let config_string = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| LineupError::LineupConfigFailure(e.to_string()))?;
    let lineup_config: LineupConfig = serde_json::from_str(&config_string)
        .map_err(|e| LineupError::LineupConfigFailure(e.to_string()))?;
    Ok(lineup_config)
}

pub fn resolve_output_folder(lineup_path: &Path, raw: &str) -> PathBuf {
    let raw_path_buf = Path::new(raw).to_path_buf();
    let expanded_path = expand_tilde(raw).unwrap_or(raw_path_buf.clone());
    if expanded_path.is_relative()
        && let Some(parent) = lineup_path.parent()
    {
        parent
            .join(&expanded_path)
            .canonicalize()
            .unwrap_or_else(|_| parent.join(&expanded_path))
    } else {
        expanded_path
    }
}
