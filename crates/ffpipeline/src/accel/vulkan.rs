use crate::ArgVec;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, SurfaceSet, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::{ScaleFilter, ToneMapFilter, VideoFilter, VideoFilterOp};

#[derive(Debug, Clone)]
pub struct Vulkan;

impl HwAccel for Vulkan {
    fn best_filter(
        &self,
        video_filter: &VideoFilter,
        ffmpeg_info: &FfmpegInfo,
        current_state: &FrameState,
    ) -> VideoFilter {
        match video_filter {
            VideoFilter::Scale(ScaleFilter {
                size,
                input_is_anamorphic,
                ..
            }) if ffmpeg_info.has_video_filter(&KnownVideoFilter::ScaleVulkan)
                && current_state.pixel_format.bit_depth() == 8 =>
            {
                ScaleVulkan {
                    size: *size,
                    input_is_anamorphic: *input_is_anamorphic,
                }
                .into()
            }
            VideoFilter::ToneMap(ToneMapFilter {
                algorithm,
                output_format: format,
            }) if ffmpeg_info.has_video_filter(&KnownVideoFilter::LibPlacebo) => LibplaceboVulkan {
                algorithm: algorithm.clone(),
                format: match format {
                    PixelFormat::Yuv420p10le => PixelFormat::P010le,
                    _ => PixelFormat::Nv12,
                },
            }
            .into(),
            _ => video_filter.clone(),
        }
    }

    fn codec_for_format(
        &self,
        format: &VideoFormat,
        _video_size: Option<FrameSize>,
    ) -> Option<VideoCodec> {
        match format {
            VideoFormat::H264 => Some(VideoCodec {
                codec_name: "h264_vulkan",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::Vulkan,
            }),
            VideoFormat::Hevc => Some(VideoCodec {
                codec_name: "hevc_vulkan",
                options: &["-tag:v", "hvc1"],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::Vulkan,
            }),
            _ => None,
        }
    }

    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter> {
        Some(
            FormatVulkan {
                format: *pixel_format,
            }
            .into(),
        )
    }

    fn init_hw_device(&self, _surfaces: &SurfaceSet) -> ArgVec {
        args!["-init_hw_device", "vulkan"]
    }

    fn known_accel(&self) -> &KnownHardwareAccel {
        &KnownHardwareAccel::Vulkan
    }

    fn make_decoder(&self, _ffmpeg_info: &FfmpegInfo, _is_hdr: bool) -> HwDecoder {
        HwDecoder {
            args: args!["-hwaccel", "vulkan", "-hwaccel_output_format", "vulkan"],
            surface: FrameSurface::Vulkan,
            filters: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct FormatVulkan {
    pub(crate) format: PixelFormat,
}

impl VideoFilterOp for FormatVulkan {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Vulkan)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!("scale_vulkan=format={}", self.format.as_arg()))
    }
}

#[derive(Clone)]
pub struct LibplaceboVulkan {
    pub(crate) algorithm: Option<String>,
    pub(crate) format: PixelFormat,
}

impl VideoFilterOp for LibplaceboVulkan {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format;
        state.is_hdr = false;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Vulkan)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!(
            "libplacebo=tonemapping={}:colorspace=bt709:color_primaries=bt709:color_trc=bt709:format={}",
            self.algorithm.as_deref().unwrap_or("linear"),
            self.format.as_arg(),
        ))
    }
}

#[derive(Clone)]
pub struct ScaleVulkan {
    pub(crate) size: Option<FrameSize>,
    pub(crate) input_is_anamorphic: bool,
}

impl VideoFilterOp for ScaleVulkan {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::Vulkan;
            state.is_anamorphic = false;
            state.sample_aspect_ratio = Some(String::from("1:1"));
            state.display_aspect_ratio = None;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Vulkan)
    }

    fn as_arg(&self) -> Option<String> {
        if let Some(size) = &self.size {
            if self.input_is_anamorphic {
                Some(format!(
                    "scale_vulkan=iw*sar:ih,setsar=1,scale_vulkan={}:{}",
                    size.width, size.height
                ))
            } else {
                Some(format!(
                    "scale_vulkan={}:{},setsar=1",
                    size.width, size.height
                ))
            }
        } else {
            None
        }
    }
}
