use std::path::{Path, PathBuf};

use ersatztv_core::{SchemaVersion, VersionedSchema};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simple_expand_tilde::expand_tilde;

use crate::error::LineupError;

const PATH_FIELDS: &[&str] = &["/output/folder", "/xmltv/folder"];

pub const SUPPORTED_SCHEMA: SchemaVersion = SchemaVersion {
    breaking: 0,
    compatible: 1,
};
pub const SCHEMA: VersionedSchema =
    VersionedSchema::new("https://ersatztv.org/lineup/version/", SUPPORTED_SCHEMA);

#[derive(Deserialize, Serialize, Clone, JsonSchema)]
pub struct LineupConfig {
    /// Schema version URI, e.g. "https://ersatztv.org/lineup/version/0.0.1"; missing is 0.0.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub version: Option<String>,
    #[serde(default = "server_config_default")]
    pub server: ServerConfig,
    pub output: OutputConfig,
    pub xmltv: Option<XmltvConfig>,
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
pub struct XmltvConfig {
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
    let mut lineup_value: serde_json::Value = serde_json::from_str(&config_string)
        .map_err(|e| LineupError::LineupConfigFailure(e.to_string()))?;
    SCHEMA.take_and_check(&mut lineup_value)?;
    let lineup_parent = path.parent().ok_or(LineupError::LineupConfigNoParent)?;
    ersatztv_core::resolve_relative_paths(&mut lineup_value, lineup_parent, PATH_FIELDS);
    let mut lineup_config: LineupConfig = serde_json::from_value(lineup_value)
        .map_err(|e| LineupError::LineupConfigFailure(e.to_string()))?;
    // a loaded config is in the current schema; add-channel writes it back
    lineup_config.version = Some(SCHEMA.uri());
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

#[cfg(test)]
mod tests {
    use ersatztv_core::SchemaVersionError;
    use serde_json::{Value, json};

    use super::*;

    async fn load(lineup: Value) -> Result<LineupConfig, LineupError> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lineup.json");
        tokio::fs::write(&path, serde_json::to_vec(&lineup).unwrap())
            .await
            .unwrap();
        from_file(&path).await
    }

    fn lineup() -> Value {
        json!({ "output": { "folder": "./hls" }, "channels": [] })
    }

    #[tokio::test]
    async fn unversioned_loads_and_serializes_as_current() {
        let config = load(lineup()).await.unwrap();

        assert_eq!(
            serde_json::to_value(&config).unwrap()["version"],
            SCHEMA.uri()
        );
    }

    #[tokio::test]
    async fn example_loads_at_current_version() {
        let example = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/lineup.json");
        let raw: Value =
            serde_json::from_str(&tokio::fs::read_to_string(&example).await.unwrap()).unwrap();
        assert_eq!(raw["version"], SCHEMA.uri());

        from_file(&example).await.unwrap();
    }

    #[tokio::test]
    async fn newer_is_rejected() {
        let mut lineup = lineup();
        lineup["version"] = json!("https://ersatztv.org/lineup/version/0.0.999");

        assert!(matches!(
            load(lineup).await,
            Err(LineupError::LineupConfigSchemaVersion(
                SchemaVersionError::Unsupported { .. }
            ))
        ));
    }
}
