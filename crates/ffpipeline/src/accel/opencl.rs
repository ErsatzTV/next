use crate::ffmpeg_info::FfmpegInfo;
use crate::frame_size::FrameSize;
use crate::pipeline::{FrameState, FrameSurface, HwPixelFormat};
use crate::video_filter::{VideoFilter, VideoFilterOp};

#[derive(Debug, Clone)]
pub struct TonemapOpencl {
    /// The algorithm to use for tonemapping.
    /// See: https://ffmpeg.org/ffmpeg-filters.html#tonemap
    pub algorithm: Option<String>,
    /// The pixel format to use for the output.
    /// Only nv12 and p010 are supported; there is no real
    /// way to only allow certain enum values of PixelFormat to be used here.
    pub output_format: HwPixelFormat,
}

impl VideoFilterOp for TonemapOpencl {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.output_format.into();
        state.is_hdr = false;
        state.surface = FrameSurface::OpenCL;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::OpenCL)
    }

    fn as_arg(&self) -> Option<String> {
        format!(
            "tonemap_opencl=tonemap={}:desat=0:t=bt709:m=bt709:p=bt709:format={}",
            self.algorithm.as_deref().unwrap_or("hable"),
            self.output_format.as_arg()
        )
        .into()
    }
}

#[derive(Debug, Clone)]
pub struct PadOpencl {
    pub(crate) size: Option<FrameSize>,
}

impl VideoFilterOp for PadOpencl {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::OpenCL;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::OpenCL)
    }

    fn as_arg(&self) -> Option<String> {
        self.size
            .as_ref()
            .map(|s| format!("pad_opencl={}:{}:-1:-1:color=black", s.width, s.height))
    }
}
