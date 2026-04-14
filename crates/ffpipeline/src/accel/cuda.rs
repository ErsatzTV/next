use crate::ffmpeg_info::{FfmpegInfo, KnownVideoFilter};
use crate::filter_chain::PipelineFilter;
use crate::frame_size::FrameSize;
use crate::hw_accel::HwAccel;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::{ForceOriginalAspectRatio, HwVideoFilter, VideoFilter};

#[derive(Debug, Clone)]
pub struct Cuda;

impl HwAccel for Cuda {
    fn best_filter(&self, video_filter: &VideoFilter, ffmpeg_info: &FfmpegInfo) -> VideoFilter {
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
            _ => video_filter.clone(),
        }
    }

    fn codec_for_format(&self, format: &VideoFormat) -> VideoCodec {
        match format {
            VideoFormat::H264 => VideoCodec {
                codec_name: "h264_nvenc",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            },
            VideoFormat::Hevc => VideoCodec {
                codec_name: "hevc_nvenc",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            },
        }
    }

    fn decoder_arg(&self) -> Vec<String> {
        vec![
            String::from("-hwaccel"),
            String::from("cuda"),
            String::from("-hwaccel_output_format"),
            String::from("cuda"),
        ]
    }

    fn decoder_filters(&self) -> Vec<PipelineFilter> {
        vec![PipelineFilter::Video(VideoFilter::Hardware(Box::new(
            HwUploadCudaWorkaround,
        )))]
    }

    fn envs(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    fn ffmpeg_name(&self) -> &str {
        "cuda"
    }

    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter> {
        Some(VideoFilter::Hardware(Box::new(FormatCuda {
            format: pixel_format.clone(),
        })))
    }

    fn frame_surface(&self) -> FrameSurface {
        FrameSurface::Cuda
    }

    fn init_hw_device(&self) -> Vec<String> {
        vec![String::from("-init_hw_device"), String::from("cuda")]
    }

    fn output_format(&self, source_pixel_format: &PixelFormat) -> PixelFormat {
        match source_pixel_format.bit_depth() {
            10 => PixelFormat::P010le,
            _ => PixelFormat::Nv12,
        }
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
