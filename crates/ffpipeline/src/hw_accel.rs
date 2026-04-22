use enum_dispatch::enum_dispatch;

use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel};
use crate::filter_chain::PipelineFilter;
use crate::frame_size::FrameSize;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::VideoFilter;
use crate::{ArgVec, accel};

#[enum_dispatch]
pub trait HwAccel {
    fn best_filter(
        &self,
        video_filter: &VideoFilter,
        _ffmpeg_info: &FfmpegInfo,
        _current_state: &FrameState,
    ) -> VideoFilter {
        video_filter.clone()
    }
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
    fn codec_for_format(
        &self,
        format: &VideoFormat,
        video_size: Option<FrameSize>,
    ) -> Option<VideoCodec>;
    fn decoder_arg(&self) -> ArgVec;
    fn decoder_filters(&self) -> Vec<PipelineFilter> {
        Vec::new()
    }
    fn decoder_frame_surface(&self) -> FrameSurface;
    fn envs(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    fn format_filter(&self, _pixel_format: &PixelFormat) -> Option<VideoFilter> {
        None
    }
    fn hw_map_filter(&self, _from: &FrameSurface, _to: &FrameSurface) -> Option<VideoFilter> {
        None
    }
    fn initialize(&self, ffmpeg_info: &FfmpegInfo, is_hdr: bool) -> Self;
    fn init_hw_device(&self) -> ArgVec;
    fn known_accel(&self) -> &KnownHardwareAccel;
    fn output_format(&self, source_pixel_format: &PixelFormat) -> PixelFormat {
        match source_pixel_format.bit_depth() {
            10 => PixelFormat::P010le,
            _ => PixelFormat::Nv12,
        }
    }
    fn supports_pixel_format(&self, _pixel_format: &PixelFormat) -> bool {
        true
    }
}

#[derive(Debug, Clone, strum::Display)]
#[enum_dispatch(HwAccel)]
#[strum(serialize_all = "lowercase")]
pub enum HardwareAccel {
    Cuda(accel::cuda::Cuda),
    Qsv(accel::qsv::Qsv),
    Vaapi(accel::vaapi::Vaapi),
    VideoToolbox(accel::video_toolbox::VideoToolbox),
    Vulkan(accel::vulkan::Vulkan),
}
