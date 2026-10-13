use std::fs;
use std::path::{Path, PathBuf};

use ersatztv::config::ChannelConfig;
use ersatztv::error::LineupError;
use serde::Deserialize;

#[derive(Deserialize, Default)]
struct ConfigHint {
    #[serde(default)]
    normalization: NormalizationHint,
}

#[derive(Deserialize, Default)]
struct NormalizationHint {
    #[serde(default)]
    video: BitrateHint,
    #[serde(default)]
    audio: BitrateHint,
    #[serde(default)]
    subtitle: SubtitleModeHint,
}

#[derive(Deserialize, Default)]
struct SubtitleModeHint {
    mode: Option<String>,
}

#[derive(Deserialize, Default)]
struct BitrateHint {
    bitrate_kbps: Option<u32>,
}

#[derive(Clone, Copy)]
pub struct StreamHints {
    pub bandwidth_bps: u32,
    pub converts_subtitles: bool,
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

    // Capture once when starting a worker; an active worker does not reload config edits.
    pub fn stream_hints(&self) -> StreamHints {
        let mut merged: serde_json::Value = serde_json::Value::Null;
        if let Ok(config_base) = fs::read_to_string(&self.config_path)
            && let Ok(value) = serde_json::from_str(&config_base)
        {
            merged = value;
        }

        for overlay_path in &self.overlay_paths {
            if let Ok(overlay_str) = fs::read_to_string(overlay_path)
                && let Ok(overlay_value) = serde_json::from_str(&overlay_str)
            {
                ersatztv_core::deep_merge(&mut merged, overlay_value);
            }
        }

        let hint = serde_json::from_value::<ConfigHint>(merged).ok();
        let converts_subtitles = hint
            .as_ref()
            .is_some_and(|h| h.normalization.subtitle.mode.as_deref() == Some("convert"));
        let bandwidth_bps = hint
            .map(|h| {
                let video = h.normalization.video.bitrate_kbps.unwrap_or(4000);
                let audio = h.normalization.audio.bitrate_kbps.unwrap_or(192);
                (video + audio) * 1100 // kbps => bps + 10% for hls overhead
            })
            .unwrap_or((4000 + 192) * 1100);

        StreamHints {
            bandwidth_bps,
            converts_subtitles,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BURN: &str = r#"{"normalization":{"subtitle":{"mode":"burn"}}}"#;
    const CONVERT: &str = r#"{"normalization":{"subtitle":{"mode":"convert"}}}"#;

    fn model(folder: &Path, base: &str, overlay: Option<&str>) -> ChannelModel {
        fs::write(folder.join("channel.json"), base).unwrap();
        let mut overlays = Vec::new();
        if let Some(overlay) = overlay {
            fs::write(folder.join("overlay.json"), overlay).unwrap();
            overlays.push(String::from("overlay.json"));
        }
        let channel = ChannelConfig {
            number: String::from("1"),
            name: String::from("One"),
            config: String::from("channel.json"),
            overlays,
            tvg_id: None,
            logo: None,
            group: None,
        };
        ChannelModel::new(&folder.join("lineup.json"), folder, channel).unwrap()
    }

    fn converts(base: &str, overlay: Option<&str>) -> bool {
        let folder = tempfile::tempdir().unwrap();
        model(folder.path(), base, overlay)
            .stream_hints()
            .converts_subtitles
    }

    #[test]
    fn subtitles_are_converted_only_in_convert_mode() {
        assert!(!converts("{}", None));
        assert!(!converts(BURN, None));
        assert!(converts(CONVERT, None));
        assert!(converts(BURN, Some(CONVERT)));
    }

    #[test]
    fn stream_hints_follow_config_edits() {
        let folder = tempfile::tempdir().unwrap();
        let channel = model(folder.path(), BURN, None);

        fs::write(
            folder.path().join("channel.json"),
            r#"{"normalization":{"subtitle":{"mode":"convert"},"video":{"bitrate_kbps":8000}}}"#,
        )
        .unwrap();
        let hints = channel.stream_hints();

        assert!(hints.converts_subtitles);
        assert_eq!(hints.bandwidth_bps, (8000 + 192) * 1100);
    }
}
