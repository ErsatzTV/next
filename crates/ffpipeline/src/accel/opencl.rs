use crate::ffmpeg_info::FfmpegInfo;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat};
use crate::video_filter::{VideoFilter, VideoFilterOp};

#[derive(Clone)]
pub struct TonemapOpencl {
    /// The algorithm to use for tonemapping.
    /// See: https://ffmpeg.org/ffmpeg-filters.html#tonemap
    pub algorithm: Option<String>,
    /// The pixel format to use for the output.
    /// Only nv12 and p010 are supported; there is no real
    /// way to only allow certain enum values of PixelFormat to be used here.
    pub output_format: PixelFormat,
}

impl VideoFilterOp for TonemapOpencl {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.output_format.clone();
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
