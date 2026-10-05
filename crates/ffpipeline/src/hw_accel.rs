use enum_dispatch::enum_dispatch;
use serde::Serialize;

use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel};
use crate::filter_chain::PipelineFilter;
use crate::frame_size::FrameSize;
use crate::output_settings::VideoFilterOptions;
use crate::overlay_filter::OverlayFilter;
use crate::pipeline::{
    EncodeFormat, EnvironmentVariable, FrameState, FrameSurface, HwPixelFormat, PixelFormat,
    SurfaceSet,
};
use crate::probe::ProbeResultVideoStream;
use crate::video_codec::{MetadataBsf, VideoCodec};
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
    /// support can depend on more than codec, profile and pixel format (e.g. interlacing)
    fn can_decode_stream(&self, video_stream: &ProbeResultVideoStream) -> bool {
        self.can_decode(
            &video_stream.codec,
            &video_stream.profile,
            &PixelFormat::parse(&video_stream.pix_fmt),
        )
    }
    fn can_encode(&self, format: &EncodeFormat, bit_depth: u8) -> bool {
        match bit_depth {
            10 => matches!(format, EncodeFormat::Hevc),
            8 => matches!(format, EncodeFormat::H264 | EncodeFormat::Hevc),
            _ => false,
        }
    }
    fn codec_for_format(
        &self,
        format: &EncodeFormat,
        bit_depth: u8,
        video_size: Option<FrameSize>,
    ) -> Option<VideoCodec>;
    /// `bt709` is only applied when the pipeline tonemaps
    fn metadata_bsf(&self, _codec: &VideoCodec) -> Option<MetadataBsf> {
        None
    }
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

    /// Returns true if the format filter of the accel (scale_vaapi, vpp_qsv, etc.) can
    /// change frames on the accel surface from pixel format `from` to pixel format `to`.
    fn can_convert_pixel_format(
        &self,
        _ffmpeg_info: &FfmpegInfo,
        _from: &PixelFormat,
        _to: &PixelFormat,
    ) -> bool {
        true
    }
}

#[derive(Debug, Clone, strum::Display, Serialize)]
#[enum_dispatch(HwAccel)]
#[strum(serialize_all = "lowercase")]
#[serde(tag = "type")]
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
