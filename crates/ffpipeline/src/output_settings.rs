use crate::frame_rate::FrameRate;
use crate::frame_size::FrameSize;
use crate::hw_accel::HardwareAccel;
use crate::output_format::OutputFormat;
use crate::pipeline::{AudioFormat, EncodeFormat, Hz, Kbps, PtsOffset, VideoFormat};

#[derive(Debug)]
pub struct OutputSettings {
    pub audio: AudioOutputSettings,
    pub video: VideoOutputSettings,
    pub accel: Option<HardwareAccel>,
    pub format: OutputFormat,
    pub pts_offset: Option<PtsOffset>,
    pub realtime: bool,
    pub is_live: bool,
    pub frame_rate: Option<FrameRate>,
    pub subtitle_mode: SubtitleMode,
    pub fonts_folder: Option<String>,
    pub subtitle_force_style: Option<String>,
    pub reports_folder: Option<String>,
    pub report_id: Option<String>,
}

/// Source codecs a stream may copy; items that can't be copied use the transcode settings.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyPolicy<T> {
    pub formats: Vec<T>,
}

impl Default for CopyPolicy<VideoFormat> {
    fn default() -> Self {
        Self {
            formats: vec![VideoFormat::H264, VideoFormat::Hevc],
        }
    }
}

/// ffprobe codec names
impl Default for CopyPolicy<String> {
    fn default() -> Self {
        Self {
            formats: ["aac", "ac3", "eac3", "mp3"].map(String::from).to_vec(),
        }
    }
}

#[derive(Debug)]
pub struct VideoOutputSettings {
    /// `None` always transcodes
    pub copy: Option<CopyPolicy<VideoFormat>>,
    pub transcode: VideoTranscodeSettings,
}

#[derive(Debug)]
pub struct VideoTranscodeSettings {
    pub format: EncodeFormat,
    pub bit_depth: u8,
    pub bitrate: Option<Kbps>,
    pub buffer: Option<Kbps>,
    pub size: Option<FrameSize>,
    pub scaling_mode: ScalingMode,
    pub deinterlace: bool,
    pub filter_options: VideoFilterOptions,
}

#[derive(Debug, Default)]
pub struct VideoFilterOptions {
    pub bwdif: BwdifOptions,
    pub bwdif_cuda: BwdifCudaOptions,
    pub deinterlace_qsv: DeinterlaceQsvOptions,
    pub deinterlace_vaapi: DeinterlaceVaapiOptions,
    pub libplacebo: LibplaceboOptions,
    pub tonemap: TonemapOptions,
    pub tonemap_opencl: TonemapOpenclOptions,
    pub w3fdif: W3fdifOptions,
    pub yadif: YadifOptions,
    pub yadif_cuda: YadifCudaOptions,
}

#[derive(Debug, Clone, Default)]
pub struct BwdifOptions {
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DeinterlaceQsvOptions {
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DeinterlaceVaapiOptions {
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BwdifCudaOptions {
    pub mode: Option<String>,
}

#[derive(Debug, Default)]
pub struct LibplaceboOptions {
    pub tonemapping: Option<String>,
}

#[derive(Debug, Default)]
pub struct TonemapOptions {
    pub tonemap: Option<String>,
}

#[derive(Debug, Default)]
pub struct TonemapOpenclOptions {
    pub tonemap: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct W3fdifOptions {
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct YadifOptions {
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct YadifCudaOptions {
    pub mode: Option<String>,
}

#[derive(Debug)]
pub struct AudioOutputSettings {
    /// `None` always transcodes
    pub copy: Option<CopyPolicy<String>>,
    pub transcode: AudioTranscodeSettings,
}

#[derive(Debug)]
pub struct AudioTranscodeSettings {
    pub format: AudioFormat,
    pub bitrate: Option<Kbps>,
    pub buffer: Option<Kbps>,
    pub channels: Option<u32>,
    pub sample_rate: Option<Hz>,
    pub loudness: Option<AudioLoudnessSettings>,
}

#[derive(Debug, Clone)]
pub struct AudioLoudnessSettings {
    pub integrated_target: f64,
    pub range_target: f64,
    pub true_peak: f64,
}

impl Default for AudioLoudnessSettings {
    fn default() -> Self {
        Self {
            integrated_target: -16f64,
            true_peak: -1.5f64,
            range_target: 11f64,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum SubtitleMode {
    Burn,
    Convert,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ScalingMode {
    ScaleAndPad,
    Stretch,
    Crop,
}
