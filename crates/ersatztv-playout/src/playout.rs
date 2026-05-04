use std::path::Path;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::{Iso8601, iso8601};

use crate::error::PlayoutError;

const DATE_CONFIG: iso8601::EncodedConfig =
    iso8601::Config::DEFAULT.set_use_separators(false).encode();
pub const DATE_FORMAT: Iso8601<DATE_CONFIG> = Iso8601::<DATE_CONFIG>;

/// A playout schedule for a single time window.
///
/// Files should be named `{start}_{finish}.json` using compact ISO 8601
/// (no separators), e.g. `20260413T000000.000000000-0500_20260414T002131.620000000-0500.json`,
/// so that the channel can locate the correct file for the current time.
#[derive(Debug, Deserialize, Serialize)]
pub struct Playout {
    /// URI identifying the schema version, e.g. "https://ersatztv.org/playout/version/0.0.1"
    pub version: String,
    pub items: Vec<PlayoutItem>,
}

impl Playout {
    pub fn new(items: Vec<PlayoutItem>) -> Self {
        Playout {
            version: String::from("https://ersatztv.org/playout/version/0.0.1"),
            items,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PlayoutItem {
    pub id: String,
    /// RFC3339 formatted date/time, e.g. 2026-04-13T00:24:21.527-05:00
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    /// RFC3339 formatted date/time, e.g. 2026-04-13T00:24:21.527-05:00
    #[serde(with = "time::serde::rfc3339")]
    pub finish: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PlayoutItemSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracks: Option<PlayoutItemTracks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark: Option<Watermark>,
}

impl PlayoutItem {
    pub fn new(
        id: String,
        start: OffsetDateTime,
        finish: OffsetDateTime,
        in_point: Option<std::time::Duration>,
        out_point: Option<std::time::Duration>,
        path: &Path,
    ) -> Result<PlayoutItem, PlayoutError> {
        Ok(PlayoutItem {
            id,
            start,
            finish,
            source: Some(PlayoutItemSource::Local {
                path: path.to_string_lossy().to_string(),
                in_point_ms: in_point.map(|d| d.as_millis() as u64),
                out_point_ms: out_point.map(|d| d.as_millis() as u64),
            }),
            tracks: None,
            watermark: None,
        })
    }

    pub fn finish(&self) -> OffsetDateTime {
        self.finish
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PlayoutItemTracks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<TrackSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<TrackSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<TrackSelection>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TrackSelection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PlayoutItemSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_index: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Watermark {
    pub source: PlayoutItemSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_index: Option<u32>,
    pub location: WatermarkLocation,
    /// Scale to this percent of primary content width (0–100).
    /// Omitted = actual size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_percent: Option<f32>,
    /// When `true`, position margins are measured from the edges of the source
    /// content rather than the padded output frame, so letterbox/pillarbox bars
    /// push the watermark inward and keep it inside the visible content. When
    /// `false`, margins are relative to the full padded frame, so a 0% margin
    /// can land inside the bars. Has no effect when the primary content fills
    /// the output (crop/stretch). Omitted = `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub within_source_content: Option<bool>,
    /// Horizontal offset from `location`, as percent of primary content width (0–100).
    /// Omitted = 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal_margin_percent: Option<f32>,
    /// Vertical offset from `location`, as percent of primary content height (0–100).
    /// Omitted = 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_margin_percent: Option<f32>,
    /// Opacity as a percent (0–100). Omitted = fully opaque (100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<WatermarkTiming>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WatermarkLocation {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "timing_type", rename_all = "snake_case")]
pub enum WatermarkTiming {
    Periodic {
        clock: PeriodicClock,
        frequency_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        phase_offset_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_after_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fade_ms: Option<u64>,
        hold_ms: u64,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodicClock {
    Wall,
    Content,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(tag = "source_type", rename_all = "snake_case")]
pub enum PlayoutItemSource {
    Local {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        in_point_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        out_point_ms: Option<u64>,
    },
    Lavfi {
        params: String,
    },
    Http {
        /// URI template, e.g. "https://example.com/file.mkv?token={{MY_SECRET}}"
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        in_point_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        out_point_ms: Option<u64>,
        /// Custom HTTP headers, e.g. ["Authorization: Bearer {{TOKEN}}"]
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<Vec<String>>,
        /// Custom user-agent string
        #[serde(skip_serializing_if = "Option::is_none")]
        user_agent: Option<String>,
        /// Socket timeout in microseconds
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_us: Option<u64>,
        /// Enable reconnect on failure (default: true)
        #[serde(skip_serializing_if = "Option::is_none")]
        reconnect: Option<bool>,
        /// Max reconnect delay in seconds
        /// Maps directly to the reconnect_delay_max ffmpeg option
        #[serde(skip_serializing_if = "Option::is_none")]
        reconnect_delay_max: Option<u32>,
    },
}

pub struct PlayoutLoadResult {
    pub playout: Playout,
    // TODO: start, finish
}

pub async fn from_file(path: &str) -> Result<PlayoutLoadResult, PlayoutError> {
    let contents = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| PlayoutError::PlayoutJsonLoadError(e.to_string()))?;

    let playout: Playout = serde_json::from_str(&contents)
        .map_err(|e| PlayoutError::PlayoutJsonLoadError(e.to_string()))?;

    Ok(PlayoutLoadResult { playout })
}

pub fn parse_playout_filename(file_stem: &str) -> Option<(OffsetDateTime, OffsetDateTime)> {
    let split: Vec<&str> = file_stem.split("_").collect();
    if split.len() == 2 {
        let maybe_start = OffsetDateTime::parse(split[0], &DATE_FORMAT)
            .ok()
            .or_else(|| parse_unix_timestamp(split[0]));

        let maybe_finish = OffsetDateTime::parse(split[1], &DATE_FORMAT)
            .ok()
            .or_else(|| parse_unix_timestamp(split[1]));

        return match (maybe_start, maybe_finish) {
            (Some(start), Some(finish)) => Some((start, finish)),
            _ => None,
        };
    }

    None
}

fn parse_unix_timestamp(timestamp: &str) -> Option<OffsetDateTime> {
    let maybe_epoch = timestamp
        .parse::<i64>()
        .map(|i| if timestamp.len() > 10 { i / 1000 } else { i });

    if let Ok(epoch) = maybe_epoch {
        OffsetDateTime::from_unix_timestamp(epoch).ok()
    } else {
        None
    }
}
