use std::path::PathBuf;

use ersatztv_core::{SchemaVersion, VersionedSchema};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use simple_expand_tilde::expand_tilde;
use time::OffsetDateTime;
use tokio::io::AsyncReadExt;

use crate::error::ChannelError;

pub const SUPPORTED_SCHEMA: SchemaVersion = SchemaVersion {
    breaking: 1,
    compatible: 1,
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

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct ChannelConfig {
    /// Schema version URI, e.g. "https://ersatztv.org/channel/version/0.1.0". Missing means 0.0.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub version: Option<String>,
    pub playout: PlayoutConfig,
    pub ffmpeg: FfmpegConfig,
    pub normalization: NormalizationConfig,
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
    pub folder: String,
    /// RFC3339 formatted date/time, e.g. 2026-04-13T00:24:21.527-05:00
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schemars(with = "Option<String>")]
    pub virtual_start: Option<OffsetDateTime>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct FfmpegConfig {
    #[serde(default, deserialize_with = "deserialize_optional_path")]
    pub ffmpeg_path: Option<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_optional_path")]
    pub ffprobe_path: Option<PathBuf>,
    #[serde(default)]
    pub disabled_filters: Vec<String>,
    #[serde(default)]
    pub preferred_filters: Vec<String>,
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
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct NormalizationConfig {
    pub audio: AudioNormalizationConfig,
    pub video: VideoNormalizationConfig,
    #[serde(default)]
    pub subtitle: SubtitleNormalizationConfig,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[schemars(title = "StreamMode")]
pub enum StreamMode {
    #[default]
    Transcode,
    Copy,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct AudioNormalizationConfig {
    /// `copy`: copy items with a source codec in `copy_formats`. Transcode all other items.
    #[serde(default)]
    pub mode: StreamMode,
    /// Source codecs to copy when `mode` is `copy`. Default: aac, ac3, eac3, mp3.
    pub copy_formats: Option<Vec<AudioCopyFormat>>,
    /// Codec for transcoded items. When `mode` is `copy`, used only for items that are not copied.
    #[serde(default)]
    pub format: AudioFormat,
    pub bitrate_kbps: Option<u32>,
    pub buffer_kbps: Option<u32>,
    pub channels: Option<u32>,
    pub sample_rate_hz: Option<u32>,
    #[serde(default)]
    pub normalize_loudness: bool,
    pub loudness: Option<AudioLoudnessConfig>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
#[schemars(title = "AudioFormat")]
pub enum AudioFormat {
    #[default]
    Aac,
    Ac3,
}

/// Limited to codecs that mux correctly into HLS MPEG-TS.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[schemars(title = "AudioCopyFormat")]
pub enum AudioCopyFormat {
    Aac,
    Ac3,
    Eac3,
    Mp2,
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
    pub integrated_target: Option<f64>,
    pub range_target: Option<f64>,
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
    /// `copy`: copy items with a source codec in `copy_formats`. Transcode all other items, and
    /// items with graphics, burned-in subtitles, still images, Dolby Vision profile 5 or AVI sources.
    #[serde(default)]
    pub mode: StreamMode,
    /// Source codecs to copy when `mode` is `copy`. Default: h264, hevc.
    pub copy_formats: Option<Vec<VideoFormat>>,
    /// Codec for transcoded items. When `mode` is `copy`, this and all other video settings apply
    /// only to items that are not copied.
    #[serde(default)]
    pub format: VideoFormat,
    #[serde(
        default = "default_bit_depth",
        deserialize_with = "deserialize_bit_depth"
    )]
    pub bit_depth: u8,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(default)]
    pub scaling_mode: ScalingMode,
    /// Output frame rate, `N` or `N/D` (e.g. `25`, `30000/1001`).
    /// Frames are dropped or repeated to match it.
    /// In `copy` mode, only items at this rate are copied. Unset keeps the source rate.
    #[serde(default, deserialize_with = "deserialize_frame_rate")]
    #[schemars(pattern(r"^[1-9][0-9]*(/[1-9][0-9]*)?$"))]
    pub frame_rate: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub buffer_kbps: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_optional_accel")]
    pub accel: Option<HardwareAccel>,
    /// Windows only. Unset picks the discrete AMD adapter.
    pub amf_device: Option<u32>,
    /// Unset uses `/dev/dri/renderD128`.
    pub vaapi_device: Option<PathBuf>,
    /// Unset lets libva select the driver for `vaapi_device`.
    pub vaapi_driver: Option<VaapiDriver>,
    #[serde(default)]
    pub deinterlace: bool,
    #[serde(default)]
    pub filters: VideoFilterOptionsConfig,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct VideoFilterOptionsConfig {
    pub bwdif: Option<BwdifOptions>,
    pub bwdif_cuda: Option<BwdifCudaOptions>,
    pub deinterlace_qsv: Option<DeinterlaceQsvOptions>,
    pub deinterlace_vaapi: Option<DeinterlaceVaapiOptions>,
    pub libplacebo: Option<LibplaceboOptions>,
    pub tonemap: Option<TonemapOptions>,
    pub tonemap_opencl: Option<TonemapOpenclOptions>,
    pub w3fdif: Option<W3fdifOptions>,
    pub yadif: Option<YadifOptions>,
    pub yadif_cuda: Option<YadifCudaOptions>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BwdifOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BwdifCudaOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeinterlaceQsvOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeinterlaceVaapiOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibplaceboOptions {
    pub tonemapping: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TonemapOptions {
    pub tonemap: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TonemapOpenclOptions {
    pub tonemap: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct W3fdifOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YadifOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YadifCudaOptions {
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
    #[default]
    #[serde(alias = "scale_and_pad")]
    ScaleAndPad,
    #[serde(alias = "stretch")]
    Stretch,
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
    #[serde(alias = "ihd", alias = "iHD")]
    Ihd,
    #[serde(alias = "i965")]
    I965,
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
    #[default]
    H264,
    Hevc,
    Mpeg2Video,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HardwareAccel {
    Amf,
    Cuda,
    Qsv,
    Rkmpp,
    Vaapi,
    VideoToolbox,
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
    #[serde(default)]
    pub mode: SubtitleMode,
    #[serde(default)]
    pub fonts_folder: Option<String>,
    #[serde(default)]
    pub force_style: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default, Copy)]
#[serde(rename_all = "lowercase")]
#[schemars(title = "SubtitleMode")]
pub enum SubtitleMode {
    #[default]
    Burn,
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
                reader.read_to_string(&mut result).await?;
                relative_to = std::env::current_dir()?;
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
                    .map_err(ChannelError::ChannelConfigIoFailure)?
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
        self.expanded_output_folder =
            expand_tilde(output_folder).ok_or(ChannelError::ChannelConfigExpandOutputFolder)?;

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
}
