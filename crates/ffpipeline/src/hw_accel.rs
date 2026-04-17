use enum_dispatch::enum_dispatch;

use crate::accel;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel};
use crate::filter_chain::PipelineFilter;
use crate::pipeline::{FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::VideoFilter;

#[enum_dispatch]
pub trait HwAccel {
    fn best_filter(&self, video_filter: &VideoFilter, ffmpeg_info: &FfmpegInfo) -> VideoFilter;
    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        match pixel_format.bit_depth() {
            10 => matches!(codec, "av1" | "hevc"),
            8 => matches!(codec, "av1" | "h264" | "hevc" | "mpeg2video"),
            _ => false,
        }
    }
    fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        match bit_depth {
            10 => matches!(format, VideoFormat::Hevc),
            8 => matches!(format, VideoFormat::H264 | VideoFormat::Hevc),
            _ => false,
        }
    }
    fn codec_for_format(&self, format: &VideoFormat) -> Option<VideoCodec>;
    fn decoder_arg(&self) -> Vec<String>;
    fn decoder_filters(&self) -> Vec<PipelineFilter>;
    fn decoder_frame_surface(&self) -> FrameSurface;
    fn encoder_frame_surface(&self) -> FrameSurface;
    fn envs(&self) -> Vec<(String, String)>;
    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter>;
    fn initialize(&self, ffmpeg_info: &FfmpegInfo, is_hdr: bool) -> Self;
    fn init_hw_device(&self) -> Vec<String>;
    fn known_accel(&self) -> &KnownHardwareAccel;
    fn output_format(&self, source_pixel_format: &PixelFormat) -> PixelFormat;
    fn supports_pixel_format(&self, _pixel_format: &PixelFormat) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
#[enum_dispatch(HwAccel)]
pub enum HardwareAccel {
    Cuda(accel::cuda::Cuda),
    Qsv(accel::qsv::Qsv),
    Vaapi(accel::vaapi::Vaapi),
    VideoToolbox(accel::video_toolbox::VideoToolbox),
}
