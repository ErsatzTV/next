use std::fs;
use std::path::{Path, PathBuf};

use ersatztv::config::ChannelConfig;
use ersatztv::error::LineupError;
use serde::Deserialize;

#[derive(Deserialize, Default)]
struct BandwidthHint {
    #[serde(default)]
    normalization: NormalizationHint,
}

#[derive(Deserialize, Default)]
struct NormalizationHint {
    #[serde(default)]
    video: BitrateHint,
    #[serde(default)]
    audio: BitrateHint,
}

#[derive(Deserialize, Default)]
struct BitrateHint {
    bitrate_kbps: Option<u32>,
}

pub struct ChannelModel {
    number: String,
    name: String,
    config_path: PathBuf,
    overlay_paths: Vec<PathBuf>,
    output_folder: PathBuf,
    tvg_id: String,
    logo: Option<String>,
    group: Option<String>,
    bandwidth_bps: u32,
}

impl ChannelModel {
    pub fn new(
        config_path: &Path,
        output_folder: &Path,
        channel: ChannelConfig,
    ) -> Result<ChannelModel, LineupError> {
        let parent = config_path
            .parent()
            .ok_or(LineupError::LineupConfigNoParent)?;

        let channel_config = ersatztv::validate_config_path(parent, &channel.config)?;

        let mut overlay_paths: Vec<PathBuf> = Vec::new();
        for overlay in &channel.overlays {
            let overlay_path = ersatztv::validate_config_path(parent, overlay)?;
            overlay_paths.push(overlay_path);
        }

        let mut merged: serde_json::Value = serde_json::Value::Null;
        if let Ok(config_base) = fs::read_to_string(&channel_config)
            && let Ok(value) = serde_json::from_str(&config_base)
        {
            merged = value;
        }

        for overlay_path in &overlay_paths {
            if let Ok(overlay_str) = fs::read_to_string(overlay_path)
                && let Ok(overlay_value) = serde_json::from_str(&overlay_str)
            {
                ersatztv_core::deep_merge(&mut merged, overlay_value);
            }
        }

        let bandwidth_bps = serde_json::from_value::<BandwidthHint>(merged)
            .ok()
            .map(|h| {
                let video = h.normalization.video.bitrate_kbps.unwrap_or(4000);
                let audio = h.normalization.audio.bitrate_kbps.unwrap_or(192);
                (video + audio) * 1100 // kbps => bps + 10% for hls overhead
            })
            .unwrap_or((4000 + 192) * 1100);

        Ok(ChannelModel {
            number: channel.number.clone(),
            name: channel.name.clone(),
            config_path: channel_config,
            overlay_paths,
            output_folder: output_folder.join(&channel.number),
            tvg_id: channel
                .tvg_id
                .unwrap_or_else(|| format!("ersatztv.{}", channel.number)),
            logo: channel.logo.clone(),
            group: channel.group.clone(),
            bandwidth_bps,
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

    pub fn bandwidth_bps(&self) -> u32 {
        self.bandwidth_bps
    }
}
