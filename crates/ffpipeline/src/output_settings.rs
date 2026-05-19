use crate::frame_rate::FrameRate;
use crate::frame_size::FrameSize;
use crate::hw_accel::HardwareAccel;
use crate::output_format::OutputFormat;
use crate::pipeline::{AudioFormat, Hz, Kbps, PtsOffset, VideoFormat};

#[derive(Debug)]
pub struct OutputSettings {
    pub audio: AudioOutputSettings,
    pub video_format: Option<VideoFormat>,
    pub bit_depth: Option<u8>,
    pub video_bitrate: Option<Kbps>,
    pub video_buffer: Option<Kbps>,
    pub video_size: Option<FrameSize>,
    pub scaling_mode: ScalingMode,
    pub filter_options: VideoFilterOptions,
    pub deinterlace: bool,
    pub accel: Option<HardwareAccel>,
    pub format: OutputFormat,
    pub pts_offset: Option<PtsOffset>,
    pub realtime: bool,
    pub is_live: bool,
    pub frame_rate: Option<FrameRate>,
    pub subtitle_mode: SubtitleMode,
    pub save_reports: bool,
    pub reports_folder: Option<String>,
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
    pub format: Option<AudioFormat>,
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
