use ersatztv_core::{
    HEARTBEAT_FILE_NAME, PLAYLIST_FILE_NAME, READY_FILE_NAME, SEGMENT_FILE_TEMPLATE,
    SUBTITLE_PLAYLIST_FILE_NAME, VersionedSchema,
};
use ffpipeline::pipeline::{KEYFRAME_INTERVAL_SECONDS, SEGMENT_SECONDS};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{AudioCopyFormat, AudioFormat, HardwareAccel, SubtitleMode, VideoFormat};
use crate::dynamic::CALLBACK_HEADERS;

/// Bump when removing, renaming or changing a field; not when adding one.
pub const DESCRIBE_VERSION: u32 = 1;

/// What this `ersatztv-channel` binary supports, printed by `ersatztv-channel describe`.
#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq)]
#[schemars(title = "Describe")]
pub struct Describe {
    /// Version of this document. Fields may be added without a bump; removing, renaming or
    /// changing the meaning of a field bumps it.
    pub describe_version: u32,
    /// The binary that produced this document.
    pub binary: BinaryInfo,
    /// Schema versions, output files and timing that integrators depend on.
    pub contract: Contract,
    /// Values the channel config accepts. A value listed here may still be unavailable on this
    /// machine or ffmpeg build.
    pub schema_capability: SchemaCapability,
    /// Probed capabilities of this machine and ffmpeg build. Always `null` for now.
    pub runtime_capability: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq)]
#[schemars(title = "BinaryInfo")]
pub struct BinaryInfo {
    /// Release version, e.g. `0.2.4+x86_64-unknown-linux-gnu`, or `<version>+local` for local builds.
    pub version: String,
    /// Full git commit hash, or `null` when the build didn't record one.
    pub commit: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq)]
#[schemars(title = "Contract")]
pub struct Contract {
    /// Playout schema version. Playout files load when their `breaking` matches and their
    /// `compatible` is not newer.
    pub playout_schema: SchemaInfo,
    /// Channel config schema version. Configs and overlays load when their `breaking` matches and
    /// their `compatible` is not newer.
    pub channel_config_schema: SchemaInfo,
    /// Files written to the channel's output folder.
    pub output_files: OutputFiles,
    /// Target HLS segment length in seconds.
    pub segment_seconds: u32,
    /// Keyframe interval in seconds.
    pub keyframe_seconds: u32,
    /// Headers sent on every dynamic source request. `DynamicSource` in the playout schema
    /// defines them.
    pub dynamic_callback_headers: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq)]
#[schemars(title = "SchemaInfo")]
pub struct SchemaInfo {
    /// Newest version URI this binary accepts, as written in a document's `version`.
    pub uri: String,
    /// Breaking digit; must match exactly.
    pub breaking: u32,
    /// Compatible digit; documents with this value or lower load.
    pub compatible: u32,
}

impl From<&VersionedSchema> for SchemaInfo {
    fn from(schema: &VersionedSchema) -> Self {
        SchemaInfo {
            uri: schema.uri(),
            breaking: schema.supported.breaking,
            compatible: schema.supported.compatible,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq)]
#[schemars(title = "OutputFiles")]
pub struct OutputFiles {
    /// HLS media playlist for the channel.
    pub playlist: String,
    /// WebVTT subtitle playlist, written when subtitles are converted.
    pub subtitle_playlist: String,
    /// printf-style segment file name, e.g. `live%06d.ts`.
    pub segment_template: String,
    /// Created once the playlist is ready to serve.
    pub ready: String,
    /// Touched by the integrator while it still wants the channel; the channel stops when it goes
    /// stale.
    pub heartbeat: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq)]
#[schemars(title = "SchemaCapability")]
pub struct SchemaCapability {
    /// Accepted `normalization.video.format` and `normalization.video.copy_formats` values.
    pub video_formats: Vec<String>,
    /// Accepted `normalization.audio.format` values.
    pub audio_formats: Vec<String>,
    /// Accepted `normalization.audio.copy_formats` values.
    pub audio_copy_formats: Vec<String>,
    /// Accepted `normalization.video.accel` values, on every platform.
    pub accels: Vec<String>,
    /// Accepted `normalization.subtitle.mode` values.
    pub subtitle_modes: Vec<String>,
}

impl Describe {
    pub fn new() -> Self {
        Describe {
            describe_version: DESCRIBE_VERSION,
            binary: BinaryInfo {
                version: ersatztv_core::VERSION.to_string(),
                commit: ersatztv_core::COMMIT.map(String::from),
            },
            contract: Contract {
                playout_schema: (&ersatztv_playout::playout::SCHEMA).into(),
                channel_config_schema: (&crate::config::SCHEMA).into(),
                output_files: OutputFiles {
                    playlist: PLAYLIST_FILE_NAME.to_string(),
                    subtitle_playlist: SUBTITLE_PLAYLIST_FILE_NAME.to_string(),
                    segment_template: SEGMENT_FILE_TEMPLATE.to_string(),
                    ready: READY_FILE_NAME.to_string(),
                    heartbeat: HEARTBEAT_FILE_NAME.to_string(),
                },
                segment_seconds: SEGMENT_SECONDS,
                keyframe_seconds: KEYFRAME_INTERVAL_SECONDS,
                dynamic_callback_headers: CALLBACK_HEADERS.map(String::from).to_vec(),
            },
            schema_capability: SchemaCapability {
                video_formats: enum_values::<VideoFormat>(),
                audio_formats: enum_values::<AudioFormat>(),
                audio_copy_formats: enum_values::<AudioCopyFormat>(),
                accels: enum_values::<HardwareAccel>(),
                subtitle_modes: enum_values::<SubtitleMode>(),
            },
            runtime_capability: None,
        }
    }
}

impl Default for Describe {
    fn default() -> Self {
        Self::new()
    }
}

/// From the schema, so the lists match what config loading accepts.
fn enum_values<T: JsonSchema>() -> Vec<String> {
    let schema = schemars::schema_for!(T);
    let schema = schema.as_value();

    let strings = |values: &Value| -> Vec<String> {
        values
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    };

    if let Some(values) = schema.get("enum") {
        return strings(values);
    }

    schema
        .get("oneOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|branch| match (branch.get("const"), branch.get("enum")) {
            (Some(Value::String(c)), _) => vec![c.clone()],
            (_, Some(values)) => strings(values),
            _ => vec![],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;

    use super::*;

    fn assert_round_trips<T: JsonSchema + DeserializeOwned>(expected: &[&str]) {
        let values = enum_values::<T>();
        assert_eq!(values, expected);
        for value in &values {
            serde_json::from_value::<T>(Value::String(value.clone()))
                .unwrap_or_else(|e| panic!("{value} doesn't deserialize: {e}"));
        }
    }

    #[test]
    fn enum_values_match_config() {
        assert_round_trips::<VideoFormat>(&["h264", "hevc", "mpeg2video"]);
        assert_round_trips::<AudioFormat>(&["aac", "ac3"]);
        assert_round_trips::<AudioCopyFormat>(&["aac", "ac3", "eac3", "mp2", "mp3"]);
        assert_round_trips::<HardwareAccel>(&[
            "amf",
            "cuda",
            "qsv",
            "rkmpp",
            "vaapi",
            "videotoolbox",
            "vulkan",
        ]);
        assert_round_trips::<SubtitleMode>(&["burn", "convert"]);
    }
}
