use crate::ArgVec;
use crate::pipeline::{FrameSurface, PixelFormat};

#[derive(Clone, PartialEq)]
pub struct VideoCodec {
    pub(crate) codec_name: &'static str,
    pub(crate) options: ArgVec,
    pub(crate) preferred_pixel_format_8bit: Option<PixelFormat>,
    pub(crate) preferred_pixel_format_10bit: Option<PixelFormat>,
    pub(crate) preferred_surface: FrameSurface,
}

impl VideoCodec {
    pub const COPY: &'static str = "copy";

    pub fn copy() -> Self {
        Self {
            codec_name: Self::COPY,
            options: Vec::new(),
            preferred_pixel_format_8bit: None,
            preferred_pixel_format_10bit: None,
            preferred_surface: FrameSurface::System,
        }
    }

    pub fn libx264() -> Self {
        Self {
            codec_name: "libx264",
            options: Vec::new(),
            preferred_pixel_format_8bit: Some(PixelFormat::Yuv420p),
            preferred_pixel_format_10bit: Some(PixelFormat::Yuv420p10le),
            preferred_surface: FrameSurface::System,
        }
    }

    pub fn libx265() -> Self {
        Self {
            codec_name: "libx265",
            options: args!["-tag:v", "hvc1", "-x265-params", "log-level=error"],
            preferred_pixel_format_8bit: Some(PixelFormat::Yuv420p),
            preferred_pixel_format_10bit: Some(PixelFormat::Yuv420p10le),
            preferred_surface: FrameSurface::System,
        }
    }

    pub(crate) fn as_arg(&self) -> ArgVec {
        let mut args = args!["-vcodec", self.codec_name];
        args.extend(self.options.iter().cloned());
        args
    }
}
