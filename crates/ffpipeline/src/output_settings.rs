use crate::frame_rate::FrameRate;
use crate::frame_size::FrameSize;
use crate::hw_accel::HardwareAccel;
use crate::output_format::OutputFormat;
use crate::pipeline::{AudioFormat, Hz, Kbps, PtsOffset, VideoFormat};

#[derive(Debug)]
pub struct OutputSettings {
    pub audio_format: Option<AudioFormat>,
    pub audio_bitrate: Option<Kbps>,
    pub audio_buffer: Option<Kbps>,
    pub audio_channels: Option<u32>,
    pub audio_sample_rate: Option<Hz>,
    pub video_format: Option<VideoFormat>,
    pub bit_depth: Option<u8>,
    pub video_bitrate: Option<Kbps>,
    pub video_buffer: Option<Kbps>,
    pub video_size: Option<FrameSize>,
    pub tonemap_algorithm: Option<String>,
    pub accel: Option<HardwareAccel>,
    pub format: OutputFormat,
    pub pts_offset: Option<PtsOffset>,
    pub realtime: bool,
    pub frame_rate: Option<FrameRate>,
}
