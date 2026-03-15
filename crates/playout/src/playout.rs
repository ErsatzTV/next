use std::fs::File;
use std::io::BufReader;

use serde::Deserialize;

use crate::error::PlayoutError;

#[derive(Debug, Deserialize)]
pub struct Playout {
    pub version: String,
    pub items: Vec<PlayoutItem>,
}

#[derive(Debug, Deserialize)]
pub struct PlayoutItem {
    pub id: String,
    pub start: String,
    pub source: Option<PlayoutItemSource>,
    pub tracks: Option<PlayoutItemTracks>,
    pub duration_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct PlayoutItemTracks {
    pub audio: Option<PlayoutItemAudioTrack>,
    pub video: Option<PlayoutItemVideoTrack>,
}

#[derive(Debug, Deserialize)]
pub struct PlayoutItemAudioTrack {
    pub source: Option<PlayoutItemSource>,
    pub stream_index: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct PlayoutItemVideoTrack {
    pub source: PlayoutItemSource,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "source_type", rename_all = "snake_case")]
pub enum PlayoutItemSource {
    Local { path: String },
    Lavfi { params: String },
}

pub struct PlayoutLoadResult {
    pub playout: Playout,
    // TODO: start, finish
}

pub fn from_file(path: &str) -> Result<PlayoutLoadResult, PlayoutError> {
    let file = File::open(path).map_err(|e| PlayoutError::PlayoutJsonLoadError(e.to_string()))?;
    let reader = BufReader::new(file);

    let playout: Playout = serde_json::from_reader(reader)
        .map_err(|e| PlayoutError::PlayoutJsonLoadError(e.to_string()))?;

    Ok(PlayoutLoadResult { playout })
}
