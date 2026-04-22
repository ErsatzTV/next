use std::borrow::Cow;

use crate::ArgVec;
use crate::pipeline::{FrameSurface, PixelFormat};

#[derive(Clone, PartialEq)]
pub struct VideoCodec {
    pub(crate) codec_name: &'static str,
    pub(crate) options: &'static [&'static str],
    pub(crate) preferred_pixel_format_8bit: Option<PixelFormat>,
    pub(crate) preferred_pixel_format_10bit: Option<PixelFormat>,
    pub(crate) preferred_surface: FrameSurface,
}

impl VideoCodec {
    pub const COPY: VideoCodec = VideoCodec {
        codec_name: "copy",
        options: &[],
        preferred_pixel_format_8bit: None,
        preferred_pixel_format_10bit: None,
        preferred_surface: FrameSurface::System,
    };

    pub const LIBX264: VideoCodec = VideoCodec {
        codec_name: "libx264",
        options: &[],
        preferred_pixel_format_8bit: Some(PixelFormat::Yuv420p),
        preferred_pixel_format_10bit: Some(PixelFormat::Yuv420p10le),
        preferred_surface: FrameSurface::System,
    };

    pub const LIBX265: VideoCodec = VideoCodec {
        codec_name: "libx265",
        options: &["-tag:v", "hvc1", "-x265-params", "log-level=error"],
        preferred_pixel_format_8bit: Some(PixelFormat::Yuv420p),
        preferred_pixel_format_10bit: Some(PixelFormat::Yuv420p10le),
        preferred_surface: FrameSurface::System,
    };

    pub(crate) fn as_arg(&self) -> ArgVec {
        let mut args = args!["-vcodec", self.codec_name];
        args.extend(self.options.iter().copied().map(Cow::Borrowed));
        args
    }
}
