use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};
use simple_expand_tilde::expand_tilde;
use time::OffsetDateTime;
use tokio::io::AsyncReadExt;

use crate::error::ChannelError;

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
}

#[derive(Deserialize, Clone, Debug, JsonSchema)]
pub struct NormalizationConfig {
    pub audio: AudioNormalizationConfig,
    pub video: VideoNormalizationConfig,
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
    pub bitrate_kbps: Option<u32>,
    pub buffer_kbps: Option<u32>,
    pub accel: Option<HardwareAccel>,
    pub vaapi_device: Option<PathBuf>,
    pub vaapi_driver: Option<VaapiDriver>,
    pub tonemap_algorithm: Option<String>,
    #[serde(default)]
    pub deinterlace: bool,
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
    Cuda,
    Qsv,
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
                                pipeline_driver.to_string().as_str(),
                            );

                        match capabilities {
                            Ok(capabilities) => {
                                log::debug!(
                                    "detected {} VAAPI entrypoints using {}",
                                    capabilities.count(),
                                    capabilities.vendor()
                                );

                                Some(ffpipeline::hw_accel::HardwareAccel::Vaapi(
                                    ffpipeline::accel::vaapi::Vaapi {
                                        device: vaapi_device.to_str()?.to_owned(),
                                        driver: vaapi_driver.clone().into(),
                                        capabilities,
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
            HardwareAccel::Vulkan => Some(ffpipeline::hw_accel::HardwareAccel::Vulkan(
                ffpipeline::accel::vulkan::Vulkan,
            )),
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

impl ChannelConfig {
    pub async fn from_stdin(
        output_folder: &PathBuf,
        number: &str,
    ) -> Result<ChannelConfig, ChannelError> {
        let mut config_string = String::new();
        let limit = 256 * 1024; // 256K
        let mut reader = tokio::io::stdin().take(limit);

        reader.read_to_string(&mut config_string).await?;

        let mut channel_config: ChannelConfig = serde_json::from_str(&config_string)
            .map_err(|e| ChannelError::ChannelConfigFailure(e.to_string()))?;

        let playout_relative_to = std::env::current_dir()?;

        channel_config.finalize(&playout_relative_to, output_folder, number)?;

        Ok(channel_config)
    }

    pub async fn from_file(
        path: &PathBuf,
        output_folder: &PathBuf,
        number: &str,
    ) -> Result<ChannelConfig, ChannelError> {
        let config_string = tokio::fs::read_to_string(path)
            .await
            .map_err(ChannelError::ChannelConfigIoFailure)?;
        let mut channel_config: ChannelConfig = serde_json::from_str(&config_string)
            .map_err(|e| ChannelError::ChannelConfigFailure(e.to_string()))?;

        let playout_relative_to =
            path.parent()
                .ok_or(ChannelError::ChannelConfigFailure(String::from(
                    "failed to find parent of config",
                )))?;

        channel_config.finalize(playout_relative_to, output_folder, number)?;

        Ok(channel_config)
    }

    fn finalize(
        &mut self,
        playout_relative_to: &Path,
        output_folder: &PathBuf,
        number: &str,
    ) -> Result<(), ChannelError> {
        if self.normalization.video.format.is_some() && self.normalization.video.bit_depth.is_none()
        {
            return Err(ChannelError::ChannelConfigFailure(String::from(
                "bit_depth is required when normalizing video",
            )));
        }

        // expand playout folder
        let playout_folder = PathBuf::from(&self.playout.folder);
        let expanded_playout_folder =
            expand_tilde(&playout_folder).ok_or(ChannelError::ChannelConfigExpandPlayoutFolder)?;
        let relative_playout_folder = if expanded_playout_folder.is_relative() {
            playout_relative_to
                .join(&expanded_playout_folder)
                .canonicalize()?
        } else {
            expanded_playout_folder
        };
        self.expanded_playout_folder = relative_playout_folder;

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
