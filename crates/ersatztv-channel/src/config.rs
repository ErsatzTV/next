use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Duration;

use ersatztv_core::{SchemaVersion, VersionedSchema};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use simple_expand_tilde::expand_tilde;
use time::OffsetDateTime;
use tokio::io::AsyncReadExt;

use crate::error::{ChannelError, IoContext};

pub const SUPPORTED_SCHEMA: SchemaVersion = SchemaVersion {
    breaking: 1,
    compatible: 2,
};
pub const SCHEMA: VersionedSchema =
    VersionedSchema::new("https://ersatztv.org/channel/version/", SUPPORTED_SCHEMA);

pub const PATH_FIELDS: &[&str] = &[
    "/playout/folder",
    "/ffmpeg/ffmpeg_path",
    "/ffmpeg/ffprobe_path",
    "/ffmpeg/reports_folder",
    "/normalization/subtitle/fonts_folder",
];

const DEFAULT_VAAPI_DEVICE: &str = "/dev/dri/renderD128";

/// Settings for one channel: where its playout is, which ffmpeg to use, and the output format.
/// A relative path in any of these settings is relative to the file that sets it.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct ChannelConfig {
    /// Schema version URI, e.g. "https://ersatztv.org/channel/version/0.1.0". Missing means 0.0.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub version: Option<String>,
    /// Where to find the channel's playout files.
    pub playout: PlayoutConfig,
    /// ffmpeg and ffprobe settings.
    pub ffmpeg: FfmpegConfig,
    /// Output format for audio, video and subtitles.
    pub normalization: NormalizationConfig,
    /// What plays when nothing is scheduled or an item fails.
    #[serde(default)]
    pub fallback: FallbackConfig,

    #[serde(skip)]
    expanded_playout_folder: PathBuf,

    #[serde(skip)]
    expanded_output_folder: PathBuf,

    #[serde(skip)]
    number: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct PlayoutConfig {
    /// Folder of playout files, named `{start}_{finish}.json`.
    pub folder: String,
    /// Play the playout as if the channel started at this time instead of now. RFC 3339, e.g.
    /// `2026-04-13T00:24:21.527-05:00`. For testing.
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schemars(with = "Option<String>")]
    pub virtual_start: Option<OffsetDateTime>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct FfmpegConfig {
    /// Path to the ffmpeg executable. Unset or empty uses `ffmpeg` from `PATH`.
    #[serde(default, deserialize_with = "deserialize_optional_path")]
    pub ffmpeg_path: Option<PathBuf>,
    /// Path to the ffprobe executable. Unset or empty uses `ffprobe` from `PATH`.
    #[serde(default, deserialize_with = "deserialize_optional_path")]
    pub ffprobe_path: Option<PathBuf>,
    /// ffmpeg video filters to never use, even when the ffmpeg build has them, e.g.
    /// `["scale_cuda"]`. Pipelines that need one use another filter or software instead.
    #[serde(default)]
    pub disabled_filters: Vec<String>,
    /// ffmpeg video filters in order of preference, used when more than one filter can do the
    /// same step, e.g. `["tonemap_vaapi", "tonemap_opencl"]`.
    #[serde(default)]
    pub preferred_filters: Vec<String>,
    /// Folder for troubleshooting reports. When set, each failed item or fallback writes a
    /// subfolder with the ffmpeg output, the channel config and media info. Unset writes none.
    #[serde(default)]
    pub reports_folder: Option<String>,
}

/// Controls the content that replaces scheduled content when there is nothing to play
/// (a gap in the playout) or when the scheduled item fails
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct FallbackConfig {
    /// Burn the reason for the fallback into the fallback video; this is always burned,
    /// regardless of the normalization subtitle mode
    #[serde(default)]
    pub show_error: bool,
    /// Fallback length in seconds when the fallback has no end time, e.g. after a failed
    /// dynamic source. Default 60. Raised to the source's `retry_budget_ms` if shorter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<NonZeroU32>,
}

impl FallbackConfig {
    pub fn duration(&self) -> Duration {
        self.duration_seconds.map_or(Duration::from_mins(1), |s| {
            Duration::from_secs(s.get().into())
        })
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct NormalizationConfig {
    /// Audio output settings.
    pub audio: AudioNormalizationConfig,
    /// Video output settings.
    pub video: VideoNormalizationConfig,
    /// Subtitle output settings.
    #[serde(default)]
    pub subtitle: SubtitleNormalizationConfig,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[schemars(title = "StreamMode")]
pub enum StreamMode {
    /// Transcode every item.
    #[default]
    Transcode,
    /// Copy items with a source codec in `copy_formats`. Transcode all other items.
    Copy,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct AudioNormalizationConfig {
    /// Whether to transcode every item or copy items whose codec allows it.
    #[serde(default)]
    pub mode: StreamMode,
    /// Source codecs to copy when `mode` is `copy`. Default: aac, ac3, eac3, mp3.
    pub copy_formats: Option<Vec<AudioCopyFormat>>,
    /// Codec for transcoded items. When `mode` is `copy`, used only for items that are not copied.
    #[serde(default)]
    pub format: AudioFormat,
    /// Bitrate in kbit/s. Unset leaves it to the encoder.
    pub bitrate_kbps: Option<u32>,
    /// Rate control buffer size (`-bufsize`) in kbit/s. Unset leaves it to the encoder.
    pub buffer_kbps: Option<u32>,
    /// Number of channels, e.g. `2` for stereo. Unset keeps the source channel count.
    pub channels: Option<u32>,
    /// Sample rate in Hz, e.g. `48000`. Unset keeps the source rate, or 48000 when
    /// `normalize_loudness` is on.
    pub sample_rate_hz: Option<u32>,
    /// Normalize loudness with the ffmpeg `loudnorm` filter to the `loudness` targets.
    /// Not allowed when `mode` is `copy`.
    #[serde(default)]
    pub normalize_loudness: bool,
    /// Targets for `normalize_loudness`.
    pub loudness: Option<AudioLoudnessConfig>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
#[schemars(title = "AudioFormat")]
pub enum AudioFormat {
    /// AAC.
    #[default]
    Aac,
    /// Dolby Digital (AC-3).
    Ac3,
}

/// Limited to codecs that mux correctly into HLS MPEG-TS.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[schemars(title = "AudioCopyFormat")]
pub enum AudioCopyFormat {
    /// AAC.
    Aac,
    /// Dolby Digital (AC-3).
    Ac3,
    /// Dolby Digital Plus (E-AC-3).
    Eac3,
    /// MPEG-1 Audio Layer II.
    Mp2,
    /// MPEG-1 Audio Layer III.
    Mp3,
}

impl AudioCopyFormat {
    pub fn codec_name(self) -> &'static str {
        match self {
            AudioCopyFormat::Aac => "aac",
            AudioCopyFormat::Ac3 => "ac3",
            AudioCopyFormat::Eac3 => "eac3",
            AudioCopyFormat::Mp2 => "mp2",
            AudioCopyFormat::Mp3 => "mp3",
        }
    }
}

impl From<AudioFormat> for ffpipeline::pipeline::AudioFormat {
    fn from(value: AudioFormat) -> Self {
        match value {
            AudioFormat::Aac => ffpipeline::pipeline::AudioFormat::Aac,
            AudioFormat::Ac3 => ffpipeline::pipeline::AudioFormat::Ac3,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct AudioLoudnessConfig {
    /// Integrated loudness target in LUFS (`loudnorm` `I`). Unset is -16.
    pub integrated_target: Option<f64>,
    /// Loudness range target in LU (`loudnorm` `LRA`). Unset is 11.
    pub range_target: Option<f64>,
    /// Maximum true peak in dBTP (`loudnorm` `TP`). Unset is -1.5.
    pub true_peak: Option<f64>,
}

impl From<&AudioLoudnessConfig> for ffpipeline::output_settings::AudioLoudnessSettings {
    fn from(value: &AudioLoudnessConfig) -> Self {
        let default_settings = Self::default();

        Self {
            integrated_target: value
                .integrated_target
                .unwrap_or(default_settings.integrated_target),
            range_target: value.range_target.unwrap_or(default_settings.range_target),
            true_peak: value.true_peak.unwrap_or(default_settings.true_peak),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct VideoNormalizationConfig {
    /// Whether to transcode every item or copy items whose codec allows it. In `copy` mode, items
    /// with graphics, burned-in subtitles, still images, Dolby Vision profile 5 or AVI sources are
    /// always transcoded.
    #[serde(default)]
    pub mode: StreamMode,
    /// Source codecs to copy when `mode` is `copy`. Default: h264, hevc.
    pub copy_formats: Option<Vec<VideoFormat>>,
    /// Codec for transcoded items. When `mode` is `copy`, this and all other video settings apply
    /// only to items that are not copied.
    #[serde(default)]
    pub format: VideoFormat,
    /// Bit depth, `8` or `10`. `mpeg2video` always uses 8.
    #[serde(
        default = "default_bit_depth",
        deserialize_with = "deserialize_bit_depth"
    )]
    pub bit_depth: u8,
    /// Width in pixels. Applies only when `height` is also set; otherwise the source size is kept.
    /// The fallback uses 1920 when unset.
    pub width: Option<u32>,
    /// Height in pixels. Applies only when `width` is also set; otherwise the source size is kept.
    /// The fallback uses 1080 when unset.
    pub height: Option<u32>,
    /// How a source with a different aspect ratio fits `width` and `height`.
    #[serde(default)]
    pub scaling_mode: ScalingMode,
    /// Output frame rate, `N` or `N/D` (e.g. `25`, `30000/1001`).
    /// Frames are dropped or repeated to match it.
    /// In `copy` mode, only items at this rate are copied. Unset keeps the source rate.
    #[serde(default, deserialize_with = "deserialize_frame_rate")]
    #[schemars(pattern(r"^[1-9][0-9]*(/[1-9][0-9]*)?$"))]
    pub frame_rate: Option<String>,
    /// Bitrate in kbit/s. Required for `mpeg2video`. Unset leaves it to the encoder.
    pub bitrate_kbps: Option<u32>,
    /// Rate control buffer size (`-bufsize`) in kbit/s. Unset leaves it to the encoder.
    pub buffer_kbps: Option<u32>,
    /// Hardware acceleration for decoding, filtering and encoding. Steps the device can't do use
    /// software. If the device can't be probed, the whole channel uses software. Unset or empty
    /// uses software.
    #[serde(default, deserialize_with = "deserialize_optional_accel")]
    pub accel: Option<HardwareAccel>,
    /// AMF adapter index. Windows only. Unset picks the discrete AMD adapter.
    pub amf_device: Option<u32>,
    /// VAAPI render node. Unset uses `/dev/dri/renderD128`.
    pub vaapi_device: Option<PathBuf>,
    /// VAAPI driver. Unset lets libva select the driver for `vaapi_device`.
    pub vaapi_driver: Option<VaapiDriver>,
    /// Deinterlace interlaced sources. Progressive sources are not changed.
    #[serde(default)]
    pub deinterlace: bool,
    /// Options for individual ffmpeg filters. Unset options use the filter's default.
    #[serde(default)]
    pub filters: VideoFilterOptionsConfig,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct VideoFilterOptionsConfig {
    /// Options for the [`bwdif`](https://ffmpeg.org/ffmpeg-filters.html#bwdif-1) filter, used for
    /// software deinterlacing.
    pub bwdif: Option<BwdifOptions>,
    /// Options for the [`bwdif_cuda`](https://ffmpeg.org/ffmpeg-filters.html#bwdif_005fcuda)
    /// filter, used for CUDA deinterlacing.
    pub bwdif_cuda: Option<BwdifCudaOptions>,
    /// Options for the `deinterlace_qsv` filter, used for QSV deinterlacing. Run
    /// `ffmpeg -h filter=deinterlace_qsv` for its options.
    pub deinterlace_qsv: Option<DeinterlaceQsvOptions>,
    /// Options for the `deinterlace_vaapi` filter, used for VAAPI deinterlacing. Run
    /// `ffmpeg -h filter=deinterlace_vaapi` for its options.
    pub deinterlace_vaapi: Option<DeinterlaceVaapiOptions>,
    /// Options for the [`libplacebo`](https://ffmpeg.org/ffmpeg-filters.html#libplacebo) filter,
    /// used for CUDA and Vulkan tonemapping.
    pub libplacebo: Option<LibplaceboOptions>,
    /// Options for the [`scale`](https://ffmpeg.org/ffmpeg-filters.html#scale-1) filter, used for
    /// software scaling.
    pub scale: Option<ScaleOptions>,
    /// Options for the [`tonemap`](https://ffmpeg.org/ffmpeg-filters.html#tonemap-1) filter, used
    /// for software tonemapping.
    pub tonemap: Option<TonemapOptions>,
    /// Options for the
    /// [`tonemap_opencl`](https://ffmpeg.org/ffmpeg-filters.html#tonemap_005fopencl) filter, used
    /// for VAAPI tonemapping.
    pub tonemap_opencl: Option<TonemapOpenclOptions>,
    /// Options for the [`w3fdif`](https://ffmpeg.org/ffmpeg-filters.html#w3fdif) filter, used for
    /// software deinterlacing.
    pub w3fdif: Option<W3fdifOptions>,
    /// Options for the [`yadif`](https://ffmpeg.org/ffmpeg-filters.html#yadif-1) filter, used for
    /// software deinterlacing.
    pub yadif: Option<YadifOptions>,
    /// Options for the [`yadif_cuda`](https://ffmpeg.org/ffmpeg-filters.html#yadif_005fcuda)
    /// filter, used for CUDA deinterlacing.
    pub yadif_cuda: Option<YadifCudaOptions>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BwdifOptions {
    /// Value for the `bwdif` `mode` option. Allowed characters: a-z, 0-9, `_`, `.`, `+`, `-`.
    #[serde(default, deserialize_with = "deserialize_filter_option")]
    #[schemars(regex(pattern = r"^[a-z0-9_.+-]+$"))]
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BwdifCudaOptions {
    /// Value for the `bwdif_cuda` `mode` option. Allowed characters: a-z, 0-9, `_`, `.`, `+`, `-`.
    #[serde(default, deserialize_with = "deserialize_filter_option")]
    #[schemars(regex(pattern = r"^[a-z0-9_.+-]+$"))]
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeinterlaceQsvOptions {
    /// Value for the `deinterlace_qsv` `mode` option. Allowed characters: a-z, 0-9, `_`, `.`, `+`,
    /// `-`.
    #[serde(default, deserialize_with = "deserialize_filter_option")]
    #[schemars(regex(pattern = r"^[a-z0-9_.+-]+$"))]
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeinterlaceVaapiOptions {
    /// Value for the `deinterlace_vaapi` `mode` option. Allowed characters: a-z, 0-9, `_`, `.`,
    /// `+`, `-`.
    #[serde(default, deserialize_with = "deserialize_filter_option")]
    #[schemars(regex(pattern = r"^[a-z0-9_.+-]+$"))]
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibplaceboOptions {
    /// Value for the `libplacebo` `tonemapping` option. Allowed characters: a-z, 0-9, `_`, `.`,
    /// `+`, `-`.
    #[serde(default, deserialize_with = "deserialize_filter_option")]
    #[schemars(regex(pattern = r"^[a-z0-9_.+-]+$"))]
    pub tonemapping: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScaleOptions {
    /// Flags for the software `scale` filter, e.g. `bicubic`, `lanczos`, `fast_bilinear`.
    /// Default is `bicubic`. Also applies to graphics, image subtitles, and hardware accels that
    /// use the software scaler.
    #[serde(default, deserialize_with = "deserialize_filter_option")]
    #[schemars(regex(pattern = r"^[a-z0-9_.+-]+$"))]
    pub flags: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TonemapOptions {
    /// Value for the `tonemap` `tonemap` option. Allowed characters: a-z, 0-9, `_`, `.`, `+`, `-`.
    #[serde(default, deserialize_with = "deserialize_filter_option")]
    #[schemars(regex(pattern = r"^[a-z0-9_.+-]+$"))]
    pub tonemap: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TonemapOpenclOptions {
    /// Value for the `tonemap_opencl` `tonemap` option. Allowed characters: a-z, 0-9, `_`, `.`,
    /// `+`, `-`.
    #[serde(default, deserialize_with = "deserialize_filter_option")]
    #[schemars(regex(pattern = r"^[a-z0-9_.+-]+$"))]
    pub tonemap: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct W3fdifOptions {
    /// Value for the `w3fdif` `mode` option. Allowed characters: a-z, 0-9, `_`, `.`, `+`, `-`.
    #[serde(default, deserialize_with = "deserialize_filter_option")]
    #[schemars(regex(pattern = r"^[a-z0-9_.+-]+$"))]
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YadifOptions {
    /// Value for the `yadif` `mode` option. Allowed characters: a-z, 0-9, `_`, `.`, `+`, `-`.
    #[serde(default, deserialize_with = "deserialize_filter_option")]
    #[schemars(regex(pattern = r"^[a-z0-9_.+-]+$"))]
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YadifCudaOptions {
    /// Value for the `yadif_cuda` `mode` option. Allowed characters: a-z, 0-9, `_`, `.`, `+`, `-`.
    #[serde(default, deserialize_with = "deserialize_filter_option")]
    #[schemars(regex(pattern = r"^[a-z0-9_.+-]+$"))]
    pub mode: Option<String>,
}

impl From<VideoFilterOptionsConfig> for ffpipeline::output_settings::VideoFilterOptions {
    fn from(value: VideoFilterOptionsConfig) -> Self {
        ffpipeline::output_settings::VideoFilterOptions {
            bwdif: ffpipeline::output_settings::BwdifOptions {
                mode: value.bwdif.and_then(|o| o.mode),
            },
            bwdif_cuda: ffpipeline::output_settings::BwdifCudaOptions {
                mode: value.bwdif_cuda.and_then(|o| o.mode),
            },
            deinterlace_qsv: ffpipeline::output_settings::DeinterlaceQsvOptions {
                mode: value.deinterlace_qsv.and_then(|o| o.mode),
            },
            deinterlace_vaapi: ffpipeline::output_settings::DeinterlaceVaapiOptions {
                mode: value.deinterlace_vaapi.and_then(|o| o.mode),
            },
            libplacebo: ffpipeline::output_settings::LibplaceboOptions {
                tonemapping: value.libplacebo.and_then(|o| o.tonemapping),
            },
            scale: ffpipeline::output_settings::ScaleOptions {
                flags: value.scale.and_then(|o| o.flags),
            },
            tonemap: ffpipeline::output_settings::TonemapOptions {
                tonemap: value.tonemap.and_then(|o| o.tonemap),
            },
            tonemap_opencl: ffpipeline::output_settings::TonemapOpenclOptions {
                tonemap: value.tonemap_opencl.and_then(|o| o.tonemap),
            },
            w3fdif: ffpipeline::output_settings::W3fdifOptions {
                mode: value.w3fdif.and_then(|o| o.mode),
            },
            yadif: ffpipeline::output_settings::YadifOptions {
                mode: value.yadif.and_then(|o| o.mode),
            },
            yadif_cuda: ffpipeline::output_settings::YadifCudaOptions {
                mode: value.yadif_cuda.and_then(|o| o.mode),
            },
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScalingMode {
    /// Scale to fit inside the frame and pad the rest with black.
    #[default]
    #[serde(alias = "scale_and_pad")]
    ScaleAndPad,
    /// Scale to fill the frame, ignoring the aspect ratio.
    #[serde(alias = "stretch")]
    Stretch,
    /// Scale to cover the frame and crop the overflow.
    #[serde(alias = "crop")]
    Crop,
}

impl From<ScalingMode> for ffpipeline::output_settings::ScalingMode {
    fn from(value: ScalingMode) -> Self {
        match value {
            ScalingMode::ScaleAndPad => ffpipeline::output_settings::ScalingMode::ScaleAndPad,
            ScalingMode::Stretch => ffpipeline::output_settings::ScalingMode::Stretch,
            ScalingMode::Crop => ffpipeline::output_settings::ScalingMode::Crop,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VaapiDriver {
    /// Intel media driver, for Broadwell and newer.
    #[serde(alias = "ihd", alias = "iHD")]
    Ihd,
    /// Legacy Intel driver, for older Intel GPUs.
    #[serde(alias = "i965")]
    I965,
    /// Mesa driver for AMD GPUs.
    #[serde(alias = "radeonsi", alias = "RadeonSI")]
    RadeonSI,
}

impl From<VaapiDriver> for ffpipeline::accel::vaapi::VaapiDriver {
    fn from(value: VaapiDriver) -> Self {
        match value {
            VaapiDriver::Ihd => ffpipeline::accel::vaapi::VaapiDriver::Ihd,
            VaapiDriver::I965 => ffpipeline::accel::vaapi::VaapiDriver::I965,
            VaapiDriver::RadeonSI => ffpipeline::accel::vaapi::VaapiDriver::RadeonSI,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[schemars(title = "VideoFormat")]
pub enum VideoFormat {
    /// H.264 (AVC).
    #[default]
    H264,
    /// H.265 (HEVC).
    Hevc,
    /// MPEG-2 video. Requires `bitrate_kbps`.
    Mpeg2Video,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HardwareAccel {
    /// AMD AMF. Windows only.
    Amf,
    /// NVIDIA NVENC/NVDEC.
    Cuda,
    /// Intel Quick Sync Video.
    Qsv,
    /// Rockchip MPP. Linux only.
    Rkmpp,
    /// VA-API. Linux only.
    Vaapi,
    /// Apple VideoToolbox. macOS only.
    VideoToolbox,
    /// Vulkan Video.
    Vulkan,
}

impl HardwareAccel {
    pub fn to_pipeline(
        &self,
        channel_config: &ChannelConfig,
    ) -> Option<ffpipeline::hw_accel::HardwareAccel> {
        match self {
            HardwareAccel::Amf => {
                let target = channel_config.normalization.video.amf_device.map_or(
                    ffpipeline::capabilities::amf::AmfDeviceTarget::Auto,
                    ffpipeline::capabilities::amf::AmfDeviceTarget::Adapter,
                );
                let capabilities =
                    ffpipeline::capabilities::amf::AmfCapabilities::probe_with(target);
                match capabilities {
                    Ok(capabilities) => {
                        log::debug!("detected AMF capabilities: {:?}", capabilities);
                        if let Some(adapter) = capabilities.adapter() {
                            log::info!(
                                "AMF will use adapter {}: {}",
                                adapter.index,
                                adapter.description
                            );
                        }
                        Some(ffpipeline::hw_accel::HardwareAccel::Amf(
                            ffpipeline::accel::amf::Amf { capabilities },
                        ))
                    }
                    Err(e) => {
                        log::error!("failed to probe AMF capabilities: {}", e);
                        None
                    }
                }
            }
            HardwareAccel::Cuda => {
                let capabilities = ffpipeline::capabilities::nvidia::NvidiaCapabilities::probe();
                match capabilities {
                    Ok(capabilities) => {
                        log::debug!("detected NVIDIA capabilities: {:?}", capabilities);
                        let vulkan =
                            ffpipeline::capabilities::vulkan::VulkanCapabilities::probe_for_nvidia(
                                capabilities.device_uuid(),
                            )
                            .inspect_err(|e| {
                                log::debug!("Vulkan unavailable for CUDA tonemapping: {e}")
                            })
                            .ok();
                        Some(ffpipeline::hw_accel::HardwareAccel::Cuda(
                            ffpipeline::accel::cuda::Cuda::new(capabilities, vulkan),
                        ))
                    }
                    Err(e) => {
                        log::error!("failed to probe NVIDIA capabilities: {}", e);
                        None
                    }
                }
            }
            HardwareAccel::Qsv => {
                let capabilities = ffpipeline::capabilities::qsv::QsvCapabilities::probe();
                match capabilities {
                    Ok(capabilities) => {
                        log::debug!("detected QSV capabilities: {capabilities}");
                        Some(ffpipeline::hw_accel::HardwareAccel::Qsv(
                            ffpipeline::accel::qsv::Qsv { capabilities },
                        ))
                    }
                    Err(e) => {
                        log::error!("failed to probe QSV capabilities: {}", e);
                        None
                    }
                }
            }
            HardwareAccel::Rkmpp => {
                let capabilities = ffpipeline::capabilities::rkmpp::RkmppCapabilities::probe();
                match capabilities {
                    Ok(capabilities) => {
                        log::debug!("detected rkmpp capabilities: {:?}", capabilities);
                        Some(ffpipeline::hw_accel::HardwareAccel::Rkmpp(
                            ffpipeline::accel::rkmpp::Rkmpp { capabilities },
                        ))
                    }
                    Err(e) => {
                        log::error!("failed to probe rkmpp capabilities: {}", e);
                        None
                    }
                }
            }
            HardwareAccel::Vaapi => {
                let video = &channel_config.normalization.video;
                let vaapi_device = video
                    .vaapi_device
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_VAAPI_DEVICE));
                if vaapi_device.exists() {
                    let driver: Option<ffpipeline::accel::vaapi::VaapiDriver> =
                        video.vaapi_driver.clone().map(Into::into);

                    let capabilities = ffpipeline::capabilities::vaapi::VaapiCapabilities::probe(
                        vaapi_device.to_str()?,
                        driver.as_ref().map(|d| d.to_string()).as_deref(),
                    );

                    match capabilities {
                        Ok(capabilities) => {
                            log::debug!(
                                "detected {} VAAPI entrypoints on {} using {}",
                                capabilities.count(),
                                vaapi_device.display(),
                                capabilities.vendor()
                            );

                            let opencl_capabilities =
                                ffpipeline::capabilities::opencl::OpenCLCapabilities::probe()
                                    .unwrap_or_default();

                            Some(ffpipeline::hw_accel::HardwareAccel::Vaapi(
                                ffpipeline::accel::vaapi::Vaapi {
                                    device: vaapi_device.to_str()?.to_owned(),
                                    driver,
                                    capabilities,
                                    opencl_capabilities,
                                },
                            ))
                        }
                        Err(e) => {
                            log::error!("failed to probe VAAPI capabilities: {}", e);
                            None
                        }
                    }
                } else {
                    log::error!(
                        "vaapi device `{}` does not exist! channel will not use hardware accel",
                        vaapi_device.display()
                    );
                    None
                }
            }
            HardwareAccel::VideoToolbox => {
                match ffpipeline::capabilities::videotoolbox::VideoToolboxCapabilities::probe() {
                    Ok(capabilities) => {
                        log::debug!("detected VideoToolbox capabilities: {:?}", capabilities);
                        Some(ffpipeline::hw_accel::HardwareAccel::VideoToolbox(
                            ffpipeline::accel::video_toolbox::VideoToolbox::new(capabilities),
                        ))
                    }
                    Err(e) => {
                        log::error!("failed to probe VideoToolbox capabilities: {}", e);
                        None
                    }
                }
            }
            HardwareAccel::Vulkan => {
                let capabilities = ffpipeline::capabilities::vulkan::VulkanCapabilities::probe();
                match capabilities {
                    Ok(capabilities) => {
                        log::debug!("detected Vulkan capabilities: {:?}", capabilities);
                        Some(ffpipeline::hw_accel::HardwareAccel::Vulkan(
                            ffpipeline::accel::vulkan::Vulkan { capabilities },
                        ))
                    }
                    Err(e) => {
                        log::error!("failed to probe Vulkan capabilities: {}", e);
                        None
                    }
                }
            }
        }
    }
}

impl From<VideoFormat> for ffpipeline::pipeline::VideoFormat {
    fn from(value: VideoFormat) -> Self {
        ffpipeline::pipeline::EncodeFormat::from(value).into()
    }
}

impl From<VideoFormat> for ffpipeline::pipeline::EncodeFormat {
    fn from(value: VideoFormat) -> Self {
        match value {
            VideoFormat::H264 => ffpipeline::pipeline::EncodeFormat::H264,
            VideoFormat::Hevc => ffpipeline::pipeline::EncodeFormat::Hevc,
            VideoFormat::Mpeg2Video => ffpipeline::pipeline::EncodeFormat::Mpeg2Video,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct SubtitleNormalizationConfig {
    /// What to do with the selected text subtitle track. Image subtitles are always burned in.
    #[serde(default)]
    pub mode: SubtitleMode,
    /// Folder of fonts for burned-in text subtitles.
    #[serde(default)]
    pub fonts_folder: Option<String>,
    /// ASS style overrides for burned-in text subtitles, e.g. `FontName=Arial,FontSize=24`
    /// (the `subtitles` filter's `force_style` option).
    #[serde(default)]
    pub force_style: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default, Copy)]
#[serde(rename_all = "lowercase")]
#[schemars(title = "SubtitleMode")]
pub enum SubtitleMode {
    /// Burn subtitles into the video.
    #[default]
    Burn,
    /// Convert text subtitles to a WebVTT track in the HLS stream.
    Convert,
}

impl From<SubtitleMode> for ffpipeline::output_settings::SubtitleMode {
    fn from(value: SubtitleMode) -> Self {
        match value {
            SubtitleMode::Burn => ffpipeline::output_settings::SubtitleMode::Burn,
            SubtitleMode::Convert => ffpipeline::output_settings::SubtitleMode::Convert,
        }
    }
}

impl ChannelConfig {
    pub async fn from_sources(
        sources: &[PathBuf],
        output_folder: &PathBuf,
        number: &str,
    ) -> Result<ChannelConfig, ChannelError> {
        let stdin_count = sources
            .iter()
            .filter(|s| s.to_str().is_some_and(|p| p == "-"))
            .count();

        if stdin_count > 1 {
            return Err(ChannelError::ChannelConfigFailure(String::from(
                "cannot load more than one channel config from stdin",
            )));
        }

        let mut config_value: Value = Value::Null;

        for config_path in sources {
            let relative_to;
            let is_stdin = config_path.to_str().is_some_and(|p| p == "-");

            let config_string = if is_stdin {
                let mut result = String::new();
                let limit = 256 * 1024; // 256K
                let mut reader = tokio::io::stdin().take(limit);
                reader
                    .read_to_string(&mut result)
                    .await
                    .io_context_named("read the channel config from", "stdin")?;
                relative_to = std::env::current_dir().io_context_named(
                    "resolve the working directory for",
                    "the channel config on stdin",
                )?;
                result
            } else {
                relative_to = config_path
                    .parent()
                    .ok_or(ChannelError::ChannelConfigFailure(String::from(
                        "failed to find parent of config",
                    )))?
                    .to_path_buf();

                tokio::fs::read_to_string(config_path)
                    .await
                    .io_context("read the channel config file", config_path)?
            };

            let mut v: Value = serde_json::from_str(config_string.as_str())
                .map_err(|e| ChannelError::ChannelConfigFailure(e.to_string()))?;

            // Check per source, not after merge: unversioned overlays must fail on a breaking bump.
            SCHEMA.take_and_check(&mut v).map_err(|error| {
                ChannelError::ChannelConfigSchemaVersion {
                    config: if is_stdin {
                        String::from("stdin")
                    } else {
                        config_path.display().to_string()
                    },
                    error,
                }
            })?;

            ersatztv_core::resolve_relative_paths(&mut v, &relative_to, PATH_FIELDS);

            ersatztv_core::deep_merge(&mut config_value, v);
        }

        let mut channel_config: ChannelConfig = serde_json::from_value(config_value)
            .map_err(|e| ChannelError::ChannelConfigFailure(e.to_string()))?;
        channel_config.version = Some(SCHEMA.uri());

        channel_config.finalize(output_folder, number)?;

        Ok(channel_config)
    }

    fn finalize(&mut self, output_folder: &PathBuf, number: &str) -> Result<(), ChannelError> {
        let audio = &self.normalization.audio;
        if audio.mode == StreamMode::Copy && audio.normalize_loudness {
            return Err(ChannelError::ChannelConfigFailure(String::from(
                "normalize_loudness is not supported with audio mode copy",
            )));
        }

        self.expanded_playout_folder = PathBuf::from(&self.playout.folder);

        // expand output folder
        self.expanded_output_folder = expand_tilde(output_folder).ok_or_else(|| {
            ChannelError::ChannelConfigExpandOutputFolder(output_folder.display().to_string())
        })?;

        self.number = number.to_owned();

        if self.normalization.video.format == VideoFormat::Mpeg2Video {
            if self.normalization.video.bitrate_kbps.is_none() {
                return Err(ChannelError::ChannelConfigFailure(String::from(
                    "bitrate_kbps is required when using mpeg2video output format",
                )));
            }

            if self.normalization.video.bit_depth == 10 {
                log::warn!("mpeg2video does not support 10-bit output, using 8-bit");
                self.normalization.video.bit_depth = 8;
            }
        }

        Ok(())
    }

    pub fn expanded_playout_folder(&self) -> &PathBuf {
        &self.expanded_playout_folder
    }

    pub fn expanded_output_folder(&self) -> &PathBuf {
        &self.expanded_output_folder
    }

    pub fn number(&self) -> &str {
        &self.number
    }
}

fn default_bit_depth() -> u8 {
    8
}

fn deserialize_bit_depth<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    match u8::deserialize(d)? {
        n @ (8 | 10) => Ok(n),
        _ => Err(serde::de::Error::custom("bit_depth must be 8 or 10")),
    }
}

fn deserialize_frame_rate<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    match Option::<String>::deserialize(d)? {
        None => Ok(None),
        Some(v) if ffpipeline::frame_rate::FrameRate::parse_target(&v).is_some() => Ok(Some(v)),
        Some(v) => Err(serde::de::Error::custom(format!(
            "frame_rate \"{v}\" must be N or N/D with positive integers, from 1 to {} fps",
            ffpipeline::frame_rate::MAX_TARGET_FRAME_RATE
        ))),
    }
}

/// The filter graph does not escape these values. A bad value must fail at load, not on every item.
fn deserialize_filter_option<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    match Option::<String>::deserialize(d)? {
        Some(v)
            if v.is_empty()
                || !v.chars().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || "_.+-".contains(c)
                }) =>
        {
            Err(serde::de::Error::custom(format!(
                "filter option \"{v}\" may only contain a-z, 0-9, _ . + and -"
            )))
        }
        v => Ok(v),
    }
}

fn deserialize_optional_path<'de, D: Deserializer<'de>>(d: D) -> Result<Option<PathBuf>, D::Error> {
    Ok(Option::<PathBuf>::deserialize(d)?.filter(|p| !p.as_os_str().is_empty()))
}

fn deserialize_optional_accel<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<HardwareAccel>, D::Error> {
    let s = Option::<String>::deserialize(d)?;
    match s.as_deref() {
        None | Some("") => Ok(None),
        Some(v) => {
            HardwareAccel::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(v))
                .map(Some)
        }
    }
}

#[cfg(test)]
mod tests {
    use ersatztv_core::SchemaVersionError;
    use serde_json::json;

    use super::*;

    fn base() -> Value {
        json!({
            "version": SCHEMA.uri(),
            "playout": { "folder": "./playout" },
            "ffmpeg": {},
            "normalization": {
                "audio": { "format": "aac" },
                "video": { "format": "h264", "bit_depth": 8 }
            }
        })
    }

    async fn load(sources: &[Value]) -> Result<ChannelConfig, ChannelError> {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for (i, source) in sources.iter().enumerate() {
            let path = dir.path().join(format!("{i}.json"));
            tokio::fs::write(&path, serde_json::to_vec(source).unwrap())
                .await
                .unwrap();
            paths.push(path);
        }
        ChannelConfig::from_sources(&paths, &dir.path().to_path_buf(), "1").await
    }

    fn config_error(result: Result<ChannelConfig, ChannelError>) -> String {
        match result {
            Err(ChannelError::ChannelConfigFailure(message)) => message,
            other => panic!("expected ChannelConfigFailure, got {:?}", other.map(|_| ())),
        }
    }

    #[tokio::test]
    async fn versioned_overlay_merges() {
        let overlay = json!({
            "version": SCHEMA.uri(),
            "normalization": { "video": { "bit_depth": 10 } }
        });

        let config = load(&[base(), overlay]).await.unwrap();

        assert_eq!(config.normalization.video.bit_depth, 10);
        assert_eq!(config.version, Some(SCHEMA.uri()));
    }

    /// Before 0.1.0, `"format": null` in an overlay meant copy. It must not load as transcode now.
    #[tokio::test]
    async fn unversioned_overlay_is_rejected() {
        let overlay = json!({ "normalization": { "video": { "format": null } } });

        match load(&[base(), overlay]).await {
            Err(ChannelError::ChannelConfigSchemaVersion { config, error }) => {
                assert!(config.ends_with("1.json"), "{config}");
                assert!(matches!(
                    error,
                    SchemaVersionError::MissingUnsupported { .. }
                ));
            }
            other => panic!("expected ChannelConfigSchemaVersion, got {:?}", other.err()),
        }
    }

    #[tokio::test]
    async fn modes_default_to_transcode() {
        let mut base = base();
        base["normalization"] = json!({ "audio": {}, "video": {} });

        let config = load(&[base]).await.unwrap();

        let audio = &config.normalization.audio;
        let video = &config.normalization.video;
        assert_eq!(audio.mode, StreamMode::Transcode);
        assert!(matches!(audio.format, AudioFormat::Aac));
        assert_eq!(video.mode, StreamMode::Transcode);
        assert_eq!(video.format, VideoFormat::H264);
        assert_eq!(video.bit_depth, 8);
    }

    #[tokio::test]
    async fn copy_mode_with_copy_formats() {
        let mut base = base();
        base["normalization"] = json!({
            "audio": { "mode": "copy", "copy_formats": ["ac3", "mp2"] },
            "video": { "mode": "copy", "copy_formats": ["mpeg2video"], "format": "hevc" }
        });

        let config = load(&[base]).await.unwrap();

        let audio = &config.normalization.audio;
        let video = &config.normalization.video;
        assert_eq!(audio.mode, StreamMode::Copy);
        assert_eq!(
            audio.copy_formats,
            Some(vec![AudioCopyFormat::Ac3, AudioCopyFormat::Mp2])
        );
        assert_eq!(video.mode, StreamMode::Copy);
        assert_eq!(video.copy_formats, Some(vec![VideoFormat::Mpeg2Video]));
        assert_eq!(video.format, VideoFormat::Hevc);
    }

    #[tokio::test]
    async fn uncopyable_format_is_rejected() {
        let mut base = base();
        base["normalization"]["audio"]["copy_formats"] = json!(["pcm_s16le"]);

        let message = config_error(load(&[base]).await);

        assert!(message.contains("pcm_s16le"), "{message}");
    }

    #[tokio::test]
    async fn null_format_is_rejected() {
        let mut base = base();
        base["normalization"]["video"]["format"] = Value::Null;

        let message = config_error(load(&[base]).await);

        assert!(message.contains("null"), "{message}");
    }

    #[tokio::test]
    async fn loudness_with_audio_copy_is_rejected() {
        let mut base = base();
        base["normalization"]["audio"] = json!({ "mode": "copy", "normalize_loudness": true });

        let message = config_error(load(&[base]).await);

        assert!(message.contains("normalize_loudness"), "{message}");
    }

    #[tokio::test]
    async fn examples_load_at_current_version() {
        for name in ["channel.json", "channel_copy.json"] {
            let example = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples")
                .join(name);
            let raw: Value =
                serde_json::from_str(&tokio::fs::read_to_string(&example).await.unwrap()).unwrap();
            assert_eq!(raw["version"], SCHEMA.uri(), "{name}");

            let output = tempfile::tempdir().unwrap();
            ChannelConfig::from_sources(&[example], &output.path().to_path_buf(), "1")
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn unsupported_overlay_is_named() {
        let overlay = json!({ "version": "https://ersatztv.org/channel/version/0.1.999" });

        match load(&[base(), overlay]).await {
            Err(ChannelError::ChannelConfigSchemaVersion { config, error }) => {
                assert!(config.ends_with("1.json"), "{config}");
                assert!(matches!(error, SchemaVersionError::Unsupported { .. }));
            }
            other => panic!("expected ChannelConfigSchemaVersion, got {:?}", other.err()),
        }
    }

    #[tokio::test]
    async fn frame_rate_accepts_rationals() {
        let mut base = base();
        base["normalization"]["video"]["frame_rate"] = json!("30000/1001");

        let config = load(&[base]).await.unwrap();

        assert_eq!(
            config.normalization.video.frame_rate.as_deref(),
            Some("30000/1001")
        );
    }

    #[tokio::test]
    async fn frame_rate_rejects_decimals() {
        let mut base = base();
        base["normalization"]["video"]["frame_rate"] = json!("29.97");

        let message = config_error(load(&[base]).await);

        assert!(
            message.contains("frame_rate \"29.97\" must be N or N/D"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn scale_flags_load() {
        let mut base = base();
        base["normalization"]["video"]["filters"]["scale"]["flags"] =
            json!("bicubic+accurate_rnd+full_chroma_int");

        let config = load(&[base]).await.unwrap();
        let options = ffpipeline::output_settings::VideoFilterOptions::from(
            config.normalization.video.filters,
        );

        assert_eq!(
            options.scale.flags.as_deref(),
            Some("bicubic+accurate_rnd+full_chroma_int")
        );
    }

    #[tokio::test]
    async fn filter_options_accept_libplacebo_names() {
        for tonemapping in ["bt.2390", "st2094-40"] {
            let mut base = base();
            base["normalization"]["video"]["filters"]["libplacebo"]["tonemapping"] =
                json!(tonemapping);

            load(&[base]).await.unwrap();
        }
    }

    #[tokio::test]
    async fn filter_options_reject_graph_syntax() {
        for (filter, option) in [
            ("scale", "flags"),
            ("yadif", "mode"),
            ("tonemap", "tonemap"),
            ("libplacebo", "tonemapping"),
        ] {
            let mut base = base();
            base["normalization"]["video"]["filters"][filter][option] =
                json!("bicubic:out_range=pc");

            let message = config_error(load(&[base]).await);

            assert!(
                message.contains("filter option \"bicubic:out_range=pc\" may only contain"),
                "{filter}.{option}: {message}"
            );
        }
    }

    #[tokio::test]
    async fn empty_filter_option_is_rejected() {
        let mut base = base();
        base["normalization"]["video"]["filters"]["scale"]["flags"] = json!("");

        let message = config_error(load(&[base]).await);

        assert!(message.contains("may only contain"), "{message}");
    }

    #[tokio::test]
    async fn fallback_duration_defaults_to_one_minute() {
        let config = load(&[base()]).await.unwrap();
        assert_eq!(config.fallback.duration(), Duration::from_mins(1));

        let mut base = base();
        base["fallback"]["duration_seconds"] = json!(30);
        let config = load(&[base]).await.unwrap();
        assert_eq!(config.fallback.duration(), Duration::from_secs(30));
    }

    #[tokio::test]
    async fn zero_fallback_duration_is_rejected() {
        let mut base = base();
        base["fallback"]["duration_seconds"] = json!(0);

        config_error(load(&[base]).await);
    }

    #[tokio::test]
    async fn a_config_file_that_cannot_be_read_is_named() {
        let folder = tempfile::tempdir().unwrap();
        let missing = folder.path().join("no-such-channel.json");
        let output = folder.path().join("out");
        let sources = [missing.clone()];

        let message = ChannelConfig::from_sources(&sources, &output, "5")
            .await
            .unwrap_err()
            .to_string();

        assert!(
            message.contains("read the channel config file"),
            "message does not name the operation: {message}"
        );
        assert!(
            message.contains(&missing.display().to_string()),
            "message does not name the file: {message}"
        );
    }
}
