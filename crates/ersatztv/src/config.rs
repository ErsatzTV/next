use schemars::JsonSchema;
use serde::Deserialize;

use crate::error::LineupError;

#[derive(Deserialize, Clone, JsonSchema)]
pub struct LineupConfig {
    #[serde(default = "server_config_default")]
    pub server: ServerConfig,
    pub output: OutputConfig,
    pub channels: Vec<ChannelConfig>,
}

#[derive(Deserialize, Clone, JsonSchema)]
pub struct ServerConfig {
    #[serde(default = "bind_address_default")]
    pub bind_address: String,
    #[serde(default = "port_default")]
    pub port: u16,
}

#[derive(Deserialize, Clone, JsonSchema)]
pub struct OutputConfig {
    pub folder: String,
}

#[derive(Deserialize, Clone, JsonSchema)]
pub struct ChannelConfig {
    pub number: String,
    pub name: String,
    pub config: String,
    pub tvg_id: Option<String>,
    pub logo: Option<String>,
    pub group: Option<String>,
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

pub async fn from_file(path: &std::path::PathBuf) -> Result<LineupConfig, LineupError> {
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
