use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::hw_accel::HwAccel;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::{HwVideoFilter, VideoFilter};

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
            VideoFilter::Scale {
                size,
                input_is_anamorphic,
                ..
            } if ffmpeg_info.has_video_filter(&KnownVideoFilter::ScaleVulkan)
                && current_state.pixel_format.bit_depth() == 8 =>
            {
                VideoFilter::Hardware(Box::new(ScaleVulkan {
                    size: size.clone(),
                    input_is_anamorphic: *input_is_anamorphic,
                    //force_original_aspect_ratio: force_original_aspect_ratio.clone(),
                }))
            }
            VideoFilter::ToneMap { algorithm, format }
                if ffmpeg_info.has_video_filter(&KnownVideoFilter::LibPlacebo) =>
            {
                VideoFilter::Hardware(Box::new(Libplacebo {
                    algorithm: algorithm.clone(),
                    format: match format {
                        PixelFormat::Yuv420p10le => PixelFormat::P010le,
                        _ => PixelFormat::Nv12,
                    },
                }))
            }
            _ => video_filter.clone(),
        }
    }

    fn codec_for_format(&self, format: &VideoFormat) -> Option<VideoCodec> {
        match format {
            VideoFormat::H264 => Some(VideoCodec {
                codec_name: "h264_vulkan",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            }),
            VideoFormat::Hevc => Some(VideoCodec {
                codec_name: "hevc_vulkan",
                options: &["-tag:v", "hvc1"],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            }),
            _ => None,
        }
    }

    fn decoder_arg(&self) -> Vec<String> {
        vec![
            String::from("-hwaccel"),
            String::from("vulkan"),
            String::from("-hwaccel_output_format"),
            String::from("vulkan"),
        ]
    }

    fn decoder_frame_surface(&self) -> FrameSurface {
        FrameSurface::Vulkan
    }

    fn encoder_frame_surface(&self) -> FrameSurface {
        FrameSurface::Vulkan
    }

    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter> {
        Some(VideoFilter::Hardware(Box::new(FormatVulkan {
            format: pixel_format.clone(),
        })))
    }

    fn initialize(&self, _ffmpeg_info: &FfmpegInfo, _is_hdr: bool) -> Self {
        self.clone()
    }

    fn init_hw_device(&self) -> Vec<String> {
        vec![String::from("-init_hw_device"), String::from("vulkan")]
    }

    fn known_accel(&self) -> &KnownHardwareAccel {
        &KnownHardwareAccel::Vulkan
    }
}

#[derive(Clone)]
struct FormatVulkan {
    format: PixelFormat,
}

impl HwVideoFilter for FormatVulkan {
    fn evaluate(&self, _state: &FrameState) -> Option<VideoFilter> {
        // called before this is used
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format.clone();
    }

    fn required_surface(&self) -> FrameSurface {
        FrameSurface::Vulkan
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!("scale_vulkan=format={}", self.format.as_arg()))
    }
}

#[derive(Clone)]
struct Libplacebo {
    algorithm: Option<String>,
    format: PixelFormat,
}

impl HwVideoFilter for Libplacebo {
    fn evaluate(&self, _state: &FrameState) -> Option<VideoFilter> {
        // called before this is used
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format.clone();
        state.is_hdr = false;
    }

    fn required_surface(&self) -> FrameSurface {
        FrameSurface::Vulkan
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
struct ScaleVulkan {
    size: Option<FrameSize>,
    input_is_anamorphic: bool,
    //force_original_aspect_ratio: Option<ForceOriginalAspectRatio>,
}

impl HwVideoFilter for ScaleVulkan {
    fn evaluate(&self, _state: &FrameState) -> Option<VideoFilter> {
        // called before this is used
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = size.clone();
            state.surface = FrameSurface::Vulkan;
            state.is_anamorphic = false;
            state.sample_aspect_ratio = Some(String::from("1:1"));
            state.display_aspect_ratio = None;
        }
    }

    fn required_surface(&self) -> FrameSurface {
        FrameSurface::Vulkan
    }

    fn as_arg(&self) -> Option<String> {
        if let Some(size) = &self.size {
            // let aspect_ratio = self
            //     .force_original_aspect_ratio
            //     .as_ref()
            //     .map_or(String::new(), |f| f.as_arg());
            //
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
