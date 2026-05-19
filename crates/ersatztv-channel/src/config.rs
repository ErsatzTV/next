use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use simple_expand_tilde::expand_tilde;
use time::OffsetDateTime;
use tokio::io::AsyncReadExt;

use crate::error::ChannelError;

pub const PATH_FIELDS: &[&str] = &[
    "/playout/folder",
    "/ffmpeg/ffmpeg_path",
    "/ffmpeg/ffprobe_path",
];

#[derive(Deserialize, Clone, Debug, JsonSchema)]
pub struct ChannelConfig {
    pub playout: PlayoutConfig,
    pub ffmpeg: FfmpegConfig,
    pub normalization: NormalizationConfig,

    #[serde(skip)]
    expanded_playout_folder: PathBuf,

    #[serde(skip)]
    expanded_output_folder: PathBuf,

    #[serde(skip)]
    number: String,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
pub struct PlayoutConfig {
    pub folder: String,
    /// RFC3339 formatted date/time, e.g. 2026-04-13T00:24:21.527-05:00
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schemars(with = "Option<String>")]
    pub virtual_start: Option<OffsetDateTime>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
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
    pub save_reports: bool,
    #[serde(default)]
    pub reports_folder: Option<String>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
pub struct NormalizationConfig {
    pub audio: AudioNormalizationConfig,
    pub video: VideoNormalizationConfig,
    #[serde(default)]
    pub subtitle: SubtitleNormalizationConfig,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
pub struct AudioNormalizationConfig {
    pub format: Option<AudioFormat>,
    pub bitrate_kbps: Option<u32>,
    pub buffer_kbps: Option<u32>,
    pub channels: Option<u32>,
    pub sample_rate_hz: Option<u32>,
    #[serde(default)]
    pub normalize_loudness: bool,
    pub loudness: Option<AudioLoudnessConfig>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Aac,
    Ac3,
}

impl From<AudioFormat> for ffpipeline::pipeline::AudioFormat {
    fn from(value: AudioFormat) -> Self {
        match value {
            AudioFormat::Aac => ffpipeline::pipeline::AudioFormat::Aac,
            AudioFormat::Ac3 => ffpipeline::pipeline::AudioFormat::Ac3,
        }
    }
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
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

#[derive(Deserialize, Clone, Debug, JsonSchema)]
pub struct VideoNormalizationConfig {
    pub format: Option<VideoFormat>,
    #[serde(default, deserialize_with = "deserialize_bit_depth")]
    pub bit_depth: Option<u8>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(default)]
    pub scaling_mode: ScalingMode,
    pub bitrate_kbps: Option<u32>,
    pub buffer_kbps: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_optional_accel")]
    pub accel: Option<HardwareAccel>,
    pub vaapi_device: Option<PathBuf>,
    pub vaapi_driver: Option<VaapiDriver>,
    #[serde(default)]
    pub deinterlace: bool,
    #[serde(default)]
    pub filters: VideoFilterOptionsConfig,
}

#[derive(Deserialize, Clone, Debug, Default, JsonSchema)]
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

#[derive(Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BwdifOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BwdifCudaOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeinterlaceQsvOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeinterlaceVaapiOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibplaceboOptions {
    pub tonemapping: Option<String>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TonemapOptions {
    pub tonemap: Option<String>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TonemapOpenclOptions {
    pub tonemap: Option<String>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct W3fdifOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YadifOptions {
    pub mode: Option<String>,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
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

#[derive(Deserialize, Clone, Copy, Debug, JsonSchema, Default)]
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

#[derive(Deserialize, Clone, Debug, JsonSchema)]
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

#[derive(Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VideoFormat {
    H264,
    Hevc,
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
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
            HardwareAccel::Amf => Some(ffpipeline::hw_accel::HardwareAccel::Amf(
                ffpipeline::accel::amf::Amf,
            )),
            HardwareAccel::Cuda => {
                let capabilities = ffpipeline::capabilities::nvidia::NvidiaCapabilities::probe();
                match capabilities {
                    Ok(capabilities) => {
                        log::debug!("detected NVIDIA capabilities: {:?}", capabilities);
                        Some(ffpipeline::hw_accel::HardwareAccel::Cuda(
                            ffpipeline::accel::cuda::Cuda::new(capabilities),
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
                        log::debug!("detected QSV capabilities: {:?}", capabilities);
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
                if let Some(vaapi_device) = &channel_config.normalization.video.vaapi_device
                    && let Some(vaapi_driver) = &channel_config.normalization.video.vaapi_driver
                {
                    if vaapi_device.exists() {
                        let pipeline_driver: ffpipeline::accel::vaapi::VaapiDriver =
                            vaapi_driver.clone().into();

                        let capabilities =
                            ffpipeline::capabilities::vaapi::VaapiCapabilities::probe(
                                vaapi_device.to_str()?,
                                Some(pipeline_driver.to_string().as_str()),
                            );

                        match capabilities {
                            Ok(capabilities) => {
                                log::debug!(
                                    "detected {} VAAPI entrypoints using {}",
                                    capabilities.count(),
                                    capabilities.vendor()
                                );

                                let opencl_capabilities =
                                    ffpipeline::capabilities::opencl::OpenCLCapabilities::probe()
                                        .unwrap_or_default();

                                Some(ffpipeline::hw_accel::HardwareAccel::Vaapi(
                                    ffpipeline::accel::vaapi::Vaapi {
                                        device: vaapi_device.to_str()?.to_owned(),
                                        driver: vaapi_driver.clone().into(),
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
                            "`vaapi_device` does not exist! channel will not use hardware accel"
                        );
                        None
                    }
                } else {
                    log::error!(
                        "hardware accel `vaapi` requires `vaapi_device` and `vaapi_driver`"
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
        match value {
            VideoFormat::H264 => ffpipeline::pipeline::VideoFormat::H264,
            VideoFormat::Hevc => ffpipeline::pipeline::VideoFormat::Hevc,
        }
    }
}

#[derive(Deserialize, Clone, Debug, JsonSchema, Default)]
pub struct SubtitleNormalizationConfig {
    #[serde(default)]
    pub mode: SubtitleMode,
}

#[derive(Deserialize, Clone, Debug, JsonSchema, Default, Copy)]
#[serde(rename_all = "lowercase")]
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

            let config_string = if config_path.to_str().is_some_and(|p| p == "-") {
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

            ersatztv_core::resolve_relative_paths(&mut v, &relative_to, PATH_FIELDS);

            ersatztv_core::deep_merge(&mut config_value, v);
        }

        let mut channel_config: ChannelConfig = serde_json::from_value(config_value)
            .map_err(|e| ChannelError::ChannelConfigFailure(e.to_string()))?;

        channel_config.finalize(output_folder, number)?;

        Ok(channel_config)
    }

    fn finalize(&mut self, output_folder: &PathBuf, number: &str) -> Result<(), ChannelError> {
        if self.normalization.video.format.is_some() && self.normalization.video.bit_depth.is_none()
        {
            return Err(ChannelError::ChannelConfigFailure(String::from(
                "bit_depth is required when normalizing video",
            )));
        }

        self.expanded_playout_folder = PathBuf::from(&self.playout.folder);

        // expand output folder
        self.expanded_output_folder =
            expand_tilde(output_folder).ok_or(ChannelError::ChannelConfigExpandOutputFolder)?;

        self.number = number.to_owned();

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

fn deserialize_bit_depth<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u8>, D::Error> {
    let bit_depth = Option::<u8>::deserialize(d)?;
    match bit_depth {
        Some(n) if ![8, 10].contains(&n) => {
            Err(serde::de::Error::custom("bit_depth must be 8 or 10"))
        }
        other => Ok(other),
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
