use crate::ArgVec;
use crate::capabilities::nvidia::NvidiaCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::filter_chain::PipelineFilter;
use crate::frame_size::FrameSize;
use crate::hw_accel::HwAccel;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::{ForceOriginalAspectRatio, HwVideoFilter, VideoFilter};

#[derive(Debug, Clone)]
pub struct Cuda {
    pub capabilities: NvidiaCapabilities,
    is_vulkan_hdr: bool,
}

impl Cuda {
    pub fn new(capabilities: NvidiaCapabilities) -> Cuda {
        Cuda {
            capabilities,
            is_vulkan_hdr: false,
        }
    }
}

impl HwAccel for Cuda {
    fn best_filter(
        &self,
        video_filter: &VideoFilter,
        ffmpeg_info: &FfmpegInfo,
        _current_state: &FrameState,
    ) -> VideoFilter {
        match video_filter {
            VideoFilter::Scale {
                size,
                input_is_anamorphic,
                force_original_aspect_ratio,
            } if ffmpeg_info.has_video_filter(&KnownVideoFilter::ScaleCuda) => {
                VideoFilter::Hardware(Box::new(ScaleCuda {
                    size: size.clone(),
                    input_is_anamorphic: *input_is_anamorphic,
                    force_original_aspect_ratio: force_original_aspect_ratio.clone(),
                }))
            }
            VideoFilter::Pad { size }
                if ffmpeg_info.has_video_filter(&KnownVideoFilter::PadCuda) =>
            {
                VideoFilter::Hardware(Box::new(PadCuda { size: size.clone() }))
            }
            VideoFilter::ToneMap { algorithm, format } if self.is_vulkan_hdr => {
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

    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        let format = match codec {
            "av1" => Some(VideoFormat::Av1),
            "h264" => Some(VideoFormat::H264),
            "hevc" => Some(VideoFormat::Hevc),
            "mpeg2video" => Some(VideoFormat::Mpeg2Video),
            "vc1" => Some(VideoFormat::Vc1),
            "vp8" => Some(VideoFormat::Vp8),
            "vp9" => Some(VideoFormat::Vp9),
            _ => None,
        };
        format.is_some_and(|f| self.capabilities.can_decode(&f, pixel_format.bit_depth()))
    }

    fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.capabilities.can_encode(format, bit_depth)
    }

    fn codec_for_format(&self, format: &VideoFormat) -> Option<VideoCodec> {
        match format {
            VideoFormat::H264 => Some(VideoCodec {
                codec_name: "h264_nvenc",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            }),
            VideoFormat::Hevc => {
                let options = if self.capabilities.b_frame_ref_mode(format) {
                    &["-tag:v", "hvc1", "-b_ref_mode", "1"]
                } else {
                    &["-tag:v", "hvc1", "-b_ref_mode", "0"]
                };

                Some(VideoCodec {
                    codec_name: "hevc_nvenc",
                    options,
                    preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                    preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                    is_hardware: true,
                })
            }
            _ => None,
        }
    }

    fn decoder_arg(&self) -> ArgVec {
        log::debug!("decoder arg, is_hdr: {}", self.is_vulkan_hdr);
        if self.is_vulkan_hdr {
            args!["-hwaccel", "vulkan", "-hwaccel_output_format", "vulkan",]
        } else {
            args!["-hwaccel", "cuda", "-hwaccel_output_format", "cuda",]
        }
    }

    fn decoder_filters(&self) -> Vec<PipelineFilter> {
        // can't work around fallback to software decode with HDR
        if self.is_vulkan_hdr {
            Vec::new()
        } else {
            vec![PipelineFilter::Video(VideoFilter::Hardware(Box::new(
                HwUploadCudaWorkaround,
            )))]
        }
    }

    fn decoder_frame_surface(&self) -> FrameSurface {
        if self.is_vulkan_hdr {
            FrameSurface::Vulkan
        } else {
            FrameSurface::Cuda
        }
    }

    fn encoder_frame_surface(&self) -> FrameSurface {
        FrameSurface::Cuda
    }

    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter> {
        Some(VideoFilter::Hardware(Box::new(FormatCuda {
            format: pixel_format.clone(),
        })))
    }

    fn initialize(&self, ffmpeg_info: &FfmpegInfo, is_hdr: bool) -> Self {
        Cuda {
            capabilities: self.capabilities.clone(),
            is_vulkan_hdr: is_hdr
                && ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Vulkan)
                && ffmpeg_info.has_video_filter(&KnownVideoFilter::LibPlacebo),
        }
    }

    fn init_hw_device(&self) -> ArgVec {
        log::debug!("init hw device, is_hdr: {}", self.is_vulkan_hdr);
        if self.is_vulkan_hdr {
            args![
                "-init_hw_device",
                "cuda=nv",
                "-init_hw_device",
                "vulkan=vk@nv"
            ]
        } else {
            args!["-init_hw_device", "cuda"]
        }
    }

    fn known_accel(&self) -> &KnownHardwareAccel {
        &KnownHardwareAccel::Cuda
    }
}

#[derive(Clone)]
struct ScaleCuda {
    size: Option<FrameSize>,
    input_is_anamorphic: bool,
    force_original_aspect_ratio: Option<ForceOriginalAspectRatio>,
}

impl HwVideoFilter for ScaleCuda {
    fn evaluate(&self, _state: &FrameState) -> Option<VideoFilter> {
        // called before this is used
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = size.clone();
            state.surface = FrameSurface::Cuda;
            state.is_anamorphic = false;
            state.sample_aspect_ratio = Some(String::from("1:1"));
            state.display_aspect_ratio = None;
        }
    }

    fn required_surface(&self) -> FrameSurface {
        FrameSurface::Cuda
    }

    fn as_arg(&self) -> Option<String> {
        if let Some(size) = &self.size {
            let aspect_ratio = self
                .force_original_aspect_ratio
                .as_ref()
                .map_or(String::new(), |f| f.as_arg());

            if self.input_is_anamorphic {
                Some(format!(
                    "scale_cuda=iw*sar:ih,setsar=1,scale_cuda={}:{}{}",
                    size.width, size.height, aspect_ratio
                ))
            } else {
                Some(format!(
                    "scale_cuda={}:{}{},setsar=1",
                    size.width, size.height, aspect_ratio
                ))
            }
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct PadCuda {
    size: Option<FrameSize>,
}

impl HwVideoFilter for PadCuda {
    fn evaluate(&self, _state: &FrameState) -> Option<VideoFilter> {
        // called before this is used
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = size.clone();
            state.surface = FrameSurface::Cuda;
        }
    }

    fn required_surface(&self) -> FrameSurface {
        FrameSurface::Cuda
    }

    fn as_arg(&self) -> Option<String> {
        self.size.as_ref().map(|s| {
            format!(
                "pad_cuda={}:{}:-1:-1:color=black,setsar=1",
                s.width, s.height
            )
        })
    }
}

#[derive(Clone)]
struct FormatCuda {
    format: PixelFormat,
}

impl HwVideoFilter for FormatCuda {
    fn evaluate(&self, _state: &FrameState) -> Option<VideoFilter> {
        // called before this is used
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format.clone();
    }

    fn required_surface(&self) -> FrameSurface {
        FrameSurface::Cuda
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!("scale_cuda=format={}", self.format.as_arg()))
    }
}

#[derive(Clone)]
struct HwUploadCudaWorkaround;

impl HwVideoFilter for HwUploadCudaWorkaround {
    fn evaluate(&self, _state: &FrameState) -> Option<VideoFilter> {
        // we always need to keep this filter
        Some(VideoFilter::Hardware(Box::new(self.clone())))
    }

    fn apply_to(&self, _state: &mut FrameState) {}

    fn required_surface(&self) -> FrameSurface {
        // saying cuda because we don't want the pipeline to download before uploading
        FrameSurface::Cuda
    }

    fn as_arg(&self) -> Option<String> {
        Some(String::from("hwupload"))
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
        state.surface = FrameSurface::Cuda;
    }

    fn required_surface(&self) -> FrameSurface {
        FrameSurface::Vulkan
    }

    fn as_arg(&self) -> Option<String> {
        let vulkan_format = match &self.format {
            PixelFormat::P010le => PixelFormat::P016,
            _ => PixelFormat::Nv12,
        };

        let cuda_format = match vulkan_format {
            PixelFormat::P016 => ",scale_cuda=format=p010",
            _ => "",
        };

        Some(format!(
            "libplacebo=tonemapping={}:colorspace=bt709:color_primaries=bt709:color_trc=bt709:format={},hwupload_cuda{}",
            self.algorithm.as_deref().unwrap_or("linear"),
            vulkan_format.as_arg(),
            cuda_format
        ))
    }
}
