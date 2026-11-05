use std::path::Path;

use ersatztv_core::{SchemaVersion, VersionedSchema};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::{Iso8601, iso8601};

use crate::error::PlayoutError;

const DATE_CONFIG: iso8601::EncodedConfig =
    iso8601::Config::DEFAULT.set_use_separators(false).encode();

pub const DATE_FORMAT: Iso8601<DATE_CONFIG> = Iso8601::<DATE_CONFIG>;

pub const SUPPORTED_SCHEMA: SchemaVersion = SchemaVersion {
    breaking: 0,
    compatible: 8,
};
pub const SCHEMA: VersionedSchema =
    VersionedSchema::new("https://ersatztv.org/playout/version/", SUPPORTED_SCHEMA);

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
            version: SCHEMA.uri(),
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
    /// Ordered graphics layers, from bottom to top. The compatibility watermark,
    /// when present, is composited below every entry in this collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphics: Vec<GraphicsLayer>,
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
                probe_hint: None,
            }),
            tracks: None,
            watermark: None,
            graphics: Vec::new(),
        })
    }

    pub fn finish(&self) -> OffsetDateTime {
        self.finish
    }

    pub fn effective_graphics(&self) -> impl Iterator<Item = &GraphicsLayer> {
        self.watermark.iter().chain(self.graphics.iter())
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
pub struct GraphicsLayer {
    pub source: PlayoutItemSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<GraphicsLayerKind>,
    pub location: GraphicsLocation,
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
    pub timing: Option<GraphicsTiming>,
    /// Canvas only: where the canvas is placed in the output frame. Omitted = full frame.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<CanvasLayout>,
}

pub type Watermark = GraphicsLayer;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CanvasLayout {
    /// Fill the output frame. A canvas with a different size is stretched.
    FullFrame,
    /// Place the canvas at `origin` without scaling. Parts outside the output are clipped.
    Region { origin: CanvasPosition },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct CanvasPosition {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicsLayerKind {
    Media,
    Canvas,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicsLocation {
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

pub type WatermarkLocation = GraphicsLocation;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "timing_type", rename_all = "snake_case")]
pub enum GraphicsTiming {
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

pub type WatermarkTiming = GraphicsTiming;

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
        #[serde(skip_serializing_if = "Option::is_none")]
        probe_hint: Option<ProbeHint>,
    },
    Lavfi {
        params: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        probe_hint: Option<ProbeHint>,
    },
    Http {
        /// URI template, e.g. "https://example.com/file.mkv?token={{MY_SECRET}}"
        uri: String,
        /// Whether the content is live and therefore cannot seek or work
        /// ahead (default: false)
        #[serde(skip_serializing_if = "Option::is_none")]
        is_live: Option<bool>,
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
        /// Enable persistent connections in ffmpeg (default: false)
        #[serde(skip_serializing_if = "Option::is_none")]
        keep_alive: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        probe_hint: Option<ProbeHint>,
    },
    Rtsp {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_us: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        probe_hint: Option<ProbeHint>,
    },
    Script {
        /// Command that writes an MPEG-TS stream to its stdout
        command: String,
        /// Optional arguments for the command
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        /// Whether the content is live and therefore cannot work ahead (default: false)
        #[serde(skip_serializing_if = "Option::is_none")]
        is_live: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        probe_hint: Option<ProbeHint>,
    },
    Dynamic {
        /// URI template, e.g. "https://example.com/file.mkv?token={{MY_SECRET}}"
        uri: String,
        /// Custom HTTP headers, e.g. ["Authorization: Bearer {{TOKEN}}"]
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<Vec<String>>,
        /// Custom user-agent string
        #[serde(skip_serializing_if = "Option::is_none")]
        user_agent: Option<String>,
        /// Socket timeout in microseconds
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_us: Option<u64>,
        /// Absent or 0: no retry
        #[serde(skip_serializing_if = "Option::is_none")]
        retry_budget_ms: Option<u64>,
        /// Default 250; doubles after each retry
        #[serde(skip_serializing_if = "Option::is_none")]
        retry_backoff_ms: Option<u64>,
    },
}

impl PlayoutItemSource {
    pub fn probe_hint(&self) -> Option<&ProbeHint> {
        match self {
            PlayoutItemSource::Local { probe_hint, .. }
            | PlayoutItemSource::Lavfi { probe_hint, .. }
            | PlayoutItemSource::Http { probe_hint, .. }
            | PlayoutItemSource::Rtsp { probe_hint, .. }
            | PlayoutItemSource::Script { probe_hint, .. } => probe_hint.as_ref(),
            PlayoutItemSource::Dynamic { .. } => None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ProbeHint {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub video: Vec<VideoHint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<AudioHint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitle: Vec<SubtitleHint>,
    pub format_name: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct VideoHint {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub pix_fmt: String,
    pub stream_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_order: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_range: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_space: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_transfer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_primaries: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dv_profile: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_hdr10_metadata: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
}

impl VideoHint {
    pub fn new(codec: String, width: u32, height: u32, pix_fmt: String) -> VideoHint {
        VideoHint {
            stream_index: 0,
            codec,
            width,
            height,
            pix_fmt,
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AudioHint {
    pub codec: String,
    pub channels: u32,
    pub stream_index: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct SubtitleHint {
    pub codec: String,
    pub stream_index: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InvalidPlayoutItem {
    pub id: String,
    pub start: OffsetDateTime,
    pub finish: OffsetDateTime,
    pub error: String,
}

#[derive(Debug)]
pub enum LoadedItem {
    Valid(Box<PlayoutItem>),
    Invalid(InvalidPlayoutItem),
}

impl LoadedItem {
    pub fn id(&self) -> &str {
        match self {
            LoadedItem::Valid(item) => &item.id,
            LoadedItem::Invalid(item) => &item.id,
        }
    }

    pub fn start(&self) -> OffsetDateTime {
        match self {
            LoadedItem::Valid(item) => item.start,
            LoadedItem::Invalid(item) => item.start,
        }
    }

    pub fn finish(&self) -> OffsetDateTime {
        match self {
            LoadedItem::Valid(item) => item.finish(),
            LoadedItem::Invalid(item) => item.finish,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemIssueKind {
    ProbeHintDropped,
    Invalid,
    /// no readable id, start or finish
    Dropped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemIssue {
    /// index in the file, not in `PlayoutLoadResult::items`
    pub index: usize,
    pub id: Option<String>,
    pub kind: ItemIssueKind,
    pub error: String,
}

pub struct PlayoutLoadResult {
    pub version: String,
    /// file order decides overlaps; dropped items are absent
    pub items: Vec<LoadedItem>,
    pub issues: Vec<ItemIssue>,
}

impl PlayoutLoadResult {
    pub fn valid_items(&self) -> impl Iterator<Item = &PlayoutItem> {
        self.items.iter().filter_map(|i| match i {
            LoadedItem::Valid(item) => Some(item.as_ref()),
            LoadedItem::Invalid(_) => None,
        })
    }
}

/// Only envelope errors (read, JSON, version, `items`) fail the whole file.
pub async fn from_file(path: &str) -> Result<PlayoutLoadResult, PlayoutError> {
    #[derive(Deserialize)]
    struct PlayoutVersion {
        version: String,
    }

    #[derive(Deserialize)]
    struct PlayoutEnvelope<'a> {
        #[serde(borrow)]
        items: Vec<&'a serde_json::value::RawValue>,
    }

    let contents = tokio::fs::read_to_string(path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            PlayoutError::PlayoutJsonDoesNotExist
        } else {
            PlayoutError::PlayoutJsonLoadError(e.to_string())
        }
    })?;

    let version_only: PlayoutVersion = serde_json::from_str(&contents)
        .map_err(|e| PlayoutError::PlayoutJsonLoadError(e.to_string()))?;

    SCHEMA.check(&version_only.version)?;

    let envelope: PlayoutEnvelope = serde_json::from_str(&contents)
        .map_err(|e| PlayoutError::PlayoutJsonLoadError(e.to_string()))?;

    let mut items = Vec::with_capacity(envelope.items.len());
    let mut issues = Vec::new();
    for (index, raw) in envelope.items.into_iter().enumerate() {
        // slow path only for items that fail
        if let Ok(item) = serde_json::from_str::<PlayoutItem>(raw.get()) {
            items.push(LoadedItem::Valid(Box::new(item)));
            continue;
        }
        if let Some(item) = load_item(index, raw.get(), &mut issues) {
            items.push(item);
        }
    }

    Ok(PlayoutLoadResult {
        version: version_only.version,
        items,
        issues,
    })
}

/// Loads one item as a playout file does: a bad probe hint is dropped and returned as the issue.
pub fn item_from_str(raw: &str) -> Result<(PlayoutItem, Option<ItemIssue>), String> {
    if let Ok(item) = serde_json::from_str::<PlayoutItem>(raw) {
        return Ok((item, None));
    }

    let mut issues = Vec::new();
    match load_item(0, raw, &mut issues) {
        Some(LoadedItem::Valid(item)) => Ok((*item, issues.pop())),
        Some(LoadedItem::Invalid(item)) => Err(item.error),
        None => Err(issues.pop().map(|i| i.error).unwrap_or_default()),
    }
}

#[derive(Deserialize)]
struct ItemSlot {
    id: String,
    #[serde(with = "time::serde::rfc3339")]
    start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    finish: OffsetDateTime,
}

fn load_item(index: usize, raw: &str, issues: &mut Vec<ItemIssue>) -> Option<LoadedItem> {
    // valid JSON can still fail as a Value (e.g. 1e400); skipped fields aren't range checked
    let mut value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(e) => {
            let error = serde_path_to_error::deserialize::<_, PlayoutItem>(
                &mut serde_json::Deserializer::from_str(raw),
            )
            .err()
            .map_or_else(|| e.to_string(), |e| e.to_string());
            return invalid_or_dropped(index, serde_json::from_str(raw).ok(), error, issues);
        }
    };

    let mut error = match parse_item(&value) {
        Ok(item) => return Some(LoadedItem::Valid(Box::new(item))),
        Err(e) => e,
    };

    // hints only skip a probe, so play the item without them
    let hints = strip_probe_hints(&mut value);
    if !hints.is_empty() {
        match parse_item(&value) {
            Ok(item) => {
                issues.push(ItemIssue {
                    index,
                    id: Some(item.id.clone()),
                    kind: ItemIssueKind::ProbeHintDropped,
                    error: probe_hint_error(&hints).unwrap_or(error),
                });
                return Some(LoadedItem::Valid(Box::new(item)));
            }
            // report the error left without hints
            Err(e) => error = e,
        }
    }

    invalid_or_dropped(index, serde_json::from_value(value).ok(), error, issues)
}

fn invalid_or_dropped(
    index: usize,
    slot: Option<ItemSlot>,
    error: String,
    issues: &mut Vec<ItemIssue>,
) -> Option<LoadedItem> {
    match slot {
        Some(slot) => {
            issues.push(ItemIssue {
                index,
                id: Some(slot.id.clone()),
                kind: ItemIssueKind::Invalid,
                error: error.clone(),
            });
            Some(LoadedItem::Invalid(InvalidPlayoutItem {
                id: slot.id,
                start: slot.start,
                finish: slot.finish,
                error,
            }))
        }
        None => {
            issues.push(ItemIssue {
                index,
                id: None,
                kind: ItemIssueKind::Dropped,
                error,
            });
            None
        }
    }
}

fn parse_item(value: &serde_json::Value) -> Result<PlayoutItem, String> {
    serde_path_to_error::deserialize(value).map_err(|e| e.to_string())
}

fn strip_probe_hints(value: &mut serde_json::Value) -> Vec<(String, serde_json::Value)> {
    fn strip(
        value: &mut serde_json::Value,
        path: &str,
        hints: &mut Vec<(String, serde_json::Value)>,
    ) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(hint) = map.remove("probe_hint") {
                    hints.push((child_path(path, "probe_hint"), hint));
                }
                for (key, child) in map.iter_mut() {
                    strip(child, &child_path(path, key), hints);
                }
            }
            serde_json::Value::Array(values) => {
                for (i, child) in values.iter_mut().enumerate() {
                    strip(child, &format!("{path}[{i}]"), hints);
                }
            }
            _ => {}
        }
    }

    fn child_path(path: &str, key: &str) -> String {
        if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}.{key}")
        }
    }

    let mut hints = Vec::new();
    strip(value, "", &mut hints);
    hints
}

/// serde loses the path inside the tagged source enum, so parse each hint alone
/// to find the bad field.
fn probe_hint_error(hints: &[(String, serde_json::Value)]) -> Option<String> {
    hints.iter().find_map(|(path, hint)| {
        serde_path_to_error::deserialize::<_, ProbeHint>(hint)
            .err()
            .map(|e| {
                if e.path().iter().next().is_some() {
                    format!("{path}.{e}")
                } else {
                    format!("{path}: {e}")
                }
            })
    })
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

#[cfg(test)]
mod tests {
    use ersatztv_core::SchemaVersionError;

    use super::*;

    fn layer(path: &str) -> serde_json::Value {
        serde_json::json!({
            "source": { "source_type": "local", "path": path },
            "location": "top_left"
        })
    }

    fn item_json() -> serde_json::Value {
        serde_json::json!({
            "id": "item",
            "start": "2026-08-14T00:00:00Z",
            "finish": "2026-08-14T00:01:00Z",
            "source": { "source_type": "lavfi", "params": "testsrc" }
        })
    }

    #[test]
    fn legacy_watermark_loads_and_graphics_defaults_empty() {
        let mut value = item_json();
        value["watermark"] = layer("legacy.png");
        let item: PlayoutItem = serde_json::from_value(value).unwrap();

        assert!(item.watermark.is_some());
        assert!(item.graphics.is_empty());
        assert!(
            !serde_json::to_value(&item)
                .unwrap()
                .get("graphics")
                .is_some()
        );
    }

    #[test]
    fn effective_graphics_orders_legacy_watermark_below_array() {
        let mut value = item_json();
        value["watermark"] = layer("legacy.png");
        value["graphics"] = serde_json::json!([layer("middle.png"), layer("top.png")]);
        let item: PlayoutItem = serde_json::from_value(value).unwrap();

        let paths: Vec<_> = item
            .effective_graphics()
            .map(|layer| match &layer.source {
                PlayoutItemSource::Local { path, .. } => path.as_str(),
                _ => panic!("expected local graphics source"),
            })
            .collect();
        assert_eq!(paths, ["legacy.png", "middle.png", "top.png"]);
    }
    #[test]
    fn graphics_kind_round_trips_and_absent_kind_stays_omitted() {
        for kind in [None, Some("media"), Some("canvas")] {
            let mut value = layer("canvas.nut");
            if let Some(kind) = kind {
                value["kind"] = serde_json::json!(kind);
            }
            let parsed: GraphicsLayer = serde_json::from_value(value.clone()).unwrap();
            match kind {
                None => assert!(parsed.kind.is_none()),
                Some("media") => assert!(matches!(parsed.kind, Some(GraphicsLayerKind::Media))),
                Some("canvas") => assert!(matches!(parsed.kind, Some(GraphicsLayerKind::Canvas))),
                _ => unreachable!(),
            }
            assert_eq!(serde_json::to_value(parsed).unwrap(), value);
        }
    }

    #[test]
    fn canvas_layout_round_trips_and_absent_layout_stays_omitted() {
        for layout in [
            None,
            Some(serde_json::json!({"mode": "full_frame"})),
            Some(serde_json::json!({"mode": "region", "origin": {"x": 80, "y": 40}})),
        ] {
            let mut value = layer("canvas.nut");
            value["kind"] = serde_json::json!("canvas");
            if let Some(layout) = &layout {
                value["layout"] = layout.clone();
            }
            let parsed: GraphicsLayer = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(parsed.layout.is_none(), layout.is_none());
            assert_eq!(serde_json::to_value(parsed).unwrap(), value);
        }
        for layout in [
            serde_json::json!({}),
            serde_json::json!({"mode": "region"}),
            serde_json::json!({"mode": "region", "origin": {"x": -1, "y": 0}}),
        ] {
            let mut value = layer("canvas.nut");
            value["layout"] = layout;
            assert!(serde_json::from_value::<GraphicsLayer>(value).is_err());
        }
    }

    #[tokio::test]
    async fn schema_003_to_007_files_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("playout.json");
        for version in ["0.0.3", "0.0.4", "0.0.5", "0.0.6", "0.0.7"] {
            let mut item = item_json();
            let mut graphics = layer("canvas.nut");
            if version == "0.0.4" {
                graphics["kind"] = serde_json::json!("canvas");
            }
            if version == "0.0.7" {
                graphics["kind"] = serde_json::json!("canvas");
                graphics["layout"] =
                    serde_json::json!({"mode": "region", "origin": {"x": 80, "y": 40}});
            }
            item["graphics"] = serde_json::json!([graphics]);
            let value = serde_json::json!({
                "version": format!("https://ersatztv.org/playout/version/{version}"),
                "items": [item]
            });
            tokio::fs::write(&path, serde_json::to_vec(&value).unwrap())
                .await
                .unwrap();
            let loaded = from_file(path.to_str().unwrap()).await.unwrap();
            assert!(loaded.issues.is_empty());
            let loaded = Playout {
                version: loaded.version,
                items: loaded
                    .items
                    .into_iter()
                    .map(|i| match i {
                        LoadedItem::Valid(item) => *item,
                        LoadedItem::Invalid(item) => panic!("unexpected invalid item {item:?}"),
                    })
                    .collect(),
            };
            assert_eq!(serde_json::to_value(loaded).unwrap(), value);
        }
    }

    #[tokio::test]
    async fn example_loads_at_current_version() {
        let example =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/playout/playout.json");
        let raw: serde_json::Value =
            serde_json::from_str(&tokio::fs::read_to_string(&example).await.unwrap()).unwrap();
        assert_eq!(raw["version"], SCHEMA.uri());

        from_file(example.to_str().unwrap()).await.unwrap();
    }

    #[tokio::test]
    async fn newer_schema_is_rejected_with_supported_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("playout.json");
        let newer = "https://ersatztv.org/playout/version/0.0.9";
        let value = serde_json::json!({ "version": newer, "items": [] });
        tokio::fs::write(&path, serde_json::to_vec(&value).unwrap())
            .await
            .unwrap();

        match from_file(path.to_str().unwrap()).await {
            Err(PlayoutError::SchemaVersion(SchemaVersionError::Unsupported {
                found,
                supported,
            })) => {
                assert_eq!(found, newer);
                assert_eq!(supported, "https://ersatztv.org/playout/version/0.0.8");
            }
            other => panic!("expected Unsupported, got {:?}", other.err()),
        }
    }

    fn timed_item(id: &str, minute: u32) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "start": format!("2026-08-14T00:{minute:02}:00Z"),
            "finish": format!("2026-08-14T00:{:02}:00Z", minute + 1),
            "source": { "source_type": "local", "path": format!("/media/{id}.mkv") }
        })
    }

    fn video_hint() -> serde_json::Value {
        serde_json::json!({
            "codec": "h264",
            "width": 1920,
            "height": 1080,
            "pix_fmt": "yuv420p",
            "stream_index": 0
        })
    }

    async fn load_items(items: Vec<serde_json::Value>) -> Result<PlayoutLoadResult, PlayoutError> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("playout.json");
        let value = serde_json::json!({ "version": SCHEMA.uri(), "items": items });
        tokio::fs::write(&path, serde_json::to_vec(&value).unwrap())
            .await
            .unwrap();
        from_file(path.to_str().unwrap()).await
    }

    fn ids(result: &PlayoutLoadResult) -> Vec<&str> {
        result.items.iter().map(LoadedItem::id).collect()
    }

    #[tokio::test]
    async fn bad_probe_hint_is_dropped_and_item_still_plays() {
        let mut bad = timed_item("bad", 1);
        let mut hint = video_hint();
        hint.as_object_mut().unwrap().remove("codec");
        bad["source"]["probe_hint"] = serde_json::json!({
            "video": [hint],
            "audio": [{ "codec": "aac", "channels": 2, "stream_index": 1 }]
        });
        let mut track_hint = timed_item("track", 2);
        track_hint["tracks"] = serde_json::json!({
            "audio": {
                "source": {
                    "source_type": "local",
                    "path": "/media/audio.mka",
                    "probe_hint": { "audio": [{ "channels": 2, "stream_index": 0 }] }
                }
            }
        });

        let result = load_items(vec![timed_item("a", 0), bad, track_hint])
            .await
            .unwrap();

        assert_eq!(ids(&result), ["a", "bad", "track"]);
        assert!(
            result
                .valid_items()
                .all(|i| i.source.as_ref().unwrap().probe_hint().is_none())
        );
        let tracks = result
            .valid_items()
            .nth(2)
            .unwrap()
            .tracks
            .as_ref()
            .unwrap();
        assert!(
            tracks
                .audio
                .as_ref()
                .unwrap()
                .source
                .as_ref()
                .unwrap()
                .probe_hint()
                .is_none()
        );

        assert_eq!(result.issues.len(), 2);
        let issue = &result.issues[0];
        assert_eq!(issue.index, 1);
        assert_eq!(issue.id.as_deref(), Some("bad"));
        assert_eq!(issue.kind, ItemIssueKind::ProbeHintDropped);
        assert!(
            issue.error.starts_with("source.probe_hint.video[0]")
                && issue.error.contains("missing field `codec`"),
            "{}",
            issue.error
        );
        assert_eq!(result.issues[1].kind, ItemIssueKind::ProbeHintDropped);
        assert_eq!(
            result.issues[1].error,
            "tracks.audio.source.probe_hint.audio[0]: missing field `codec`"
        );
    }

    #[tokio::test]
    async fn invalid_item_keeps_its_slot_in_file_order() {
        let mut bad = timed_item("bad", 1);
        bad["source"] = serde_json::json!({ "source_type": "carrier_pigeon" });

        let result = load_items(vec![timed_item("a", 0), bad, timed_item("c", 2)])
            .await
            .unwrap();

        assert_eq!(ids(&result), ["a", "bad", "c"]);
        let LoadedItem::Invalid(invalid) = &result.items[1] else {
            panic!("expected invalid item");
        };
        assert_eq!(invalid.start, result.items[0].finish());
        assert_eq!(invalid.finish, result.items[2].start());
        assert!(
            invalid.error.contains("carrier_pigeon"),
            "{}",
            invalid.error
        );
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].kind, ItemIssueKind::Invalid);
        assert_eq!(result.issues[0].error, invalid.error);
    }

    #[tokio::test]
    async fn invalid_item_reports_the_error_left_after_dropping_hints() {
        let mut bad = timed_item("bad", 1);
        bad["source"]["probe_hint"] = serde_json::json!({ "video": [{}] });
        bad["source"].as_object_mut().unwrap().remove("path");

        let result = load_items(vec![bad]).await.unwrap();

        let LoadedItem::Invalid(invalid) = &result.items[0] else {
            panic!("expected invalid item");
        };
        assert!(
            invalid.error.contains("missing field `path`"),
            "{}",
            invalid.error
        );
    }

    #[tokio::test]
    async fn item_without_a_readable_slot_is_dropped() {
        let mut bad = timed_item("bad", 1);
        bad["start"] = serde_json::json!("yesterday");

        let result = load_items(vec![timed_item("a", 0), bad, timed_item("c", 2)])
            .await
            .unwrap();

        assert_eq!(ids(&result), ["a", "c"]);
        assert_eq!(
            result.issues,
            [ItemIssue {
                index: 1,
                id: None,
                kind: ItemIssueKind::Dropped,
                error: result.issues[0].error.clone(),
            }]
        );
        assert!(
            result.issues[0].error.starts_with("start"),
            "{}",
            result.issues[0].error
        );
    }

    #[tokio::test]
    async fn out_of_range_number_fails_only_its_item() {
        let mut bad = timed_item("bad", 1);
        bad["source"]["probe_hint"] = serde_json::json!({ "duration_ms": 123456789 });
        let items = vec![timed_item("a", 0), bad, timed_item("c", 2)];
        let contents = serde_json::json!({ "version": SCHEMA.uri(), "items": items })
            .to_string()
            .replace("123456789", "1e400");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("playout.json");
        tokio::fs::write(&path, contents).await.unwrap();

        let result = from_file(path.to_str().unwrap()).await.unwrap();

        assert_eq!(ids(&result), ["a", "bad", "c"]);
        let LoadedItem::Invalid(invalid) = &result.items[1] else {
            panic!("expected invalid item");
        };
        assert!(
            invalid.error.contains("number out of range"),
            "{}",
            invalid.error
        );
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].kind, ItemIssueKind::Invalid);
    }

    #[tokio::test]
    async fn envelope_errors_still_fail_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("playout.json");
        for contents in [
            serde_json::json!({ "version": SCHEMA.uri() }).to_string(),
            serde_json::json!({ "version": SCHEMA.uri(), "items": {} }).to_string(),
            String::from("{ \"version\": "),
        ] {
            tokio::fs::write(&path, &contents).await.unwrap();
            assert!(
                matches!(
                    from_file(path.to_str().unwrap()).await,
                    Err(PlayoutError::PlayoutJsonLoadError(_))
                ),
                "{contents}"
            );
        }
    }
}
