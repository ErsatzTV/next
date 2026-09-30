use crate::ArgVec;
use crate::pipeline::{FrameSurface, PixelFormat};

/// Header fixups applied after encode with `h264_metadata` / `hevc_metadata`
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetadataBsf {
    pub(crate) filter: &'static str,
    pub(crate) square_pixels: bool,
    pub(crate) bt709: bool,
}

impl MetadataBsf {
    pub(crate) fn is_empty(&self) -> bool {
        !self.square_pixels && !self.bt709
    }

    pub(crate) fn as_arg(&self) -> ArgVec {
        let mut params = Vec::new();
        if self.square_pixels {
            params.push("sample_aspect_ratio=1/1");
        }
        if self.bt709 {
            params.push("colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1");
        }
        args!["-bsf:v", format!("{}={}", self.filter, params.join(":"))]
    }
}

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

    pub fn codec_name(&self) -> &'static str {
        self.codec_name
    }

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
