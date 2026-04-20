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
    pub tonemap_algorithm: Option<String>,
    pub deinterlace: bool,
    pub accel: Option<HardwareAccel>,
    pub format: OutputFormat,
    pub pts_offset: Option<PtsOffset>,
    pub realtime: bool,
    pub frame_rate: Option<FrameRate>,
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
