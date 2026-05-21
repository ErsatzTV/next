use enum_dispatch::enum_dispatch;

use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel};
use crate::filter_chain::PipelineFilter;
use crate::frame_size::FrameSize;
use crate::output_settings::VideoFilterOptions;
use crate::overlay_filter::OverlayFilter;
use crate::pipeline::{
    EnvironmentVariable, FrameState, FrameSurface, HwPixelFormat, PixelFormat, SurfaceSet,
    VideoFormat,
};
use crate::probe::ProbeResultVideoStream;
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
        _filter_options: &VideoFilterOptions,
    ) -> VideoFilter {
        video_filter.clone()
    }
    fn best_overlay(
        &self,
        overlay_filter: &OverlayFilter,
        _ffmpeg_info: &FfmpegInfo,
        _current_state: &FrameState,
    ) -> OverlayFilter {
        overlay_filter.clone()
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
        bit_depth: u8,
        video_size: Option<FrameSize>,
    ) -> Option<VideoCodec>;
    fn envs(&self) -> Vec<EnvironmentVariable> {
        Vec::new()
    }
    fn format_filter(&self, _pixel_format: &PixelFormat) -> Option<VideoFilter> {
        None
    }
    fn hw_map_filter(&self, _from: &FrameSurface, _to: &FrameSurface) -> Option<VideoFilter> {
        None
    }
    fn init_hw_device(&self, surfaces: &SurfaceSet) -> ArgVec;
    fn known_accel(&self) -> Option<&KnownHardwareAccel>;
    fn make_decoder(
        &self,
        ffmpeg_info: &FfmpegInfo,
        video_stream: &ProbeResultVideoStream,
    ) -> Option<HwDecoder>;
    fn output_format(&self, source_pixel_format: &PixelFormat) -> HwPixelFormat {
        match source_pixel_format.bit_depth() {
            10 => HwPixelFormat::P010le,
            _ => HwPixelFormat::Nv12,
        }
    }

    /// Can hwupload be used for this pixel format on the accel's surface
    fn accepts_upload_format(&self, _pixel_format: &PixelFormat) -> bool {
        true
    }

    /// Can the accel's format filter (scale_vaapi, vpp_qsv, etc.) use this pixel format
    fn can_convert_pixel_format(&self, _pixel_format: &PixelFormat) -> bool {
        true
    }
}

#[derive(Debug, Clone, strum::Display)]
#[enum_dispatch(HwAccel)]
#[strum(serialize_all = "lowercase")]
pub enum HardwareAccel {
    Amf(accel::amf::Amf),
    Cuda(accel::cuda::Cuda),
    Qsv(accel::qsv::Qsv),
    Rkmpp(accel::rkmpp::Rkmpp),
    Vaapi(accel::vaapi::Vaapi),
    VideoToolbox(accel::video_toolbox::VideoToolbox),
    Vulkan(accel::vulkan::Vulkan),
}

pub struct HwDecoder {
    pub args: ArgVec,
    pub filters: Vec<PipelineFilter>,
    pub surface: FrameSurface,
}
