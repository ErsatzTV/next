use std::path::PathBuf;

use serde::{Deserialize, Deserializer};
use simple_expand_tilde::expand_tilde;
use time::OffsetDateTime;

use crate::error::ChannelError;

#[derive(Deserialize, Clone, Debug)]
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

#[derive(Deserialize, Clone, Debug)]
pub struct PlayoutConfig {
    pub folder: String,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub virtual_start: Option<OffsetDateTime>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct FfmpegConfig {
    pub ffmpeg_path: Option<PathBuf>,
    pub ffprobe_path: Option<PathBuf>,
    #[serde(default)]
    pub disabled_filters: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct NormalizationConfig {
    pub audio: AudioNormalizationConfig,
    pub video: VideoNormalizationConfig,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AudioNormalizationConfig {
    pub format: Option<AudioFormat>,
    pub bitrate_kbps: Option<u32>,
    pub buffer_kbps: Option<u32>,
    pub channels: Option<u32>,
}

#[derive(Deserialize, Clone, Debug)]
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

#[derive(Deserialize, Clone, Debug)]
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
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum VaapiDriver {
    Ihd,
    I965,
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

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum VideoFormat {
    H264,
    Hevc,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum HardwareAccel {
    Cuda,
    Qsv,
    Vaapi,
    VideoToolbox,
}

impl HardwareAccel {
    pub(crate) fn to_pipeline(
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
                            ffpipeline::accel::cuda::Cuda { capabilities },
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
            HardwareAccel::VideoToolbox => Some(ffpipeline::hw_accel::HardwareAccel::VideoToolbox(
                ffpipeline::accel::video_toolbox::VideoToolbox,
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
    pub async fn from_file(
        path: &PathBuf,
        output_folder: &PathBuf,
        number: &str,
    ) -> Result<ChannelConfig, ChannelError> {
        // load and deserialize
        let config_string = tokio::fs::read_to_string(path)
            .await
            .map_err(ChannelError::ChannelConfigIoFailure)?;
        let mut channel_config: ChannelConfig = toml::from_str(&config_string)
            .map_err(|e| ChannelError::ChannelConfigFailure(e.to_string()))?;

        if channel_config.normalization.video.format.is_some()
            && channel_config.normalization.video.bit_depth.is_none()
        {
            return Err(ChannelError::ChannelConfigFailure(String::from(
                "bit_depth is required when normalizing video",
            )));
        }

        // expand playout folder
        let playout_folder = PathBuf::from(&channel_config.playout.folder);
        let mut expanded_playout_folder =
            expand_tilde(&playout_folder).ok_or(ChannelError::ChannelConfigExpandPlayoutFolder)?;
        if expanded_playout_folder.is_relative() {
            let parent = path
                .parent()
                .ok_or(ChannelError::ChannelConfigFailure(String::from(
                    "failed to find parent of config",
                )))?;
            expanded_playout_folder = parent.join(&expanded_playout_folder).canonicalize()?;
        }
        channel_config.expanded_playout_folder = expanded_playout_folder;

        // expand output folder
        channel_config.expanded_output_folder =
            expand_tilde(output_folder).ok_or(ChannelError::ChannelConfigExpandOutputFolder)?;

        channel_config.number = number.to_owned();

        Ok(channel_config)
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
