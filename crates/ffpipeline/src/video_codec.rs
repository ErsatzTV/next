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
    /// on `preferred_surface`, passed to the encoder without conversion
    pub(crate) also_accepts: &'static [PixelFormat],
}

impl VideoCodec {
    pub fn codec_name(&self) -> &'static str {
        self.codec_name
    }

    pub fn libx264() -> Self {
        Self {
            codec_name: "libx264",
            options: Vec::new(),
            preferred_pixel_format_8bit: Some(PixelFormat::Yuv420p),
            preferred_pixel_format_10bit: Some(PixelFormat::Yuv420p10le),
            preferred_surface: FrameSurface::System,
            also_accepts: &[],
        }
    }

    pub fn libx265() -> Self {
        Self {
            codec_name: "libx265",
            options: args!["-tag:v", "hvc1", "-x265-params", "log-level=error"],
            preferred_pixel_format_8bit: Some(PixelFormat::Yuv420p),
            preferred_pixel_format_10bit: Some(PixelFormat::Yuv420p10le),
            preferred_surface: FrameSurface::System,
            also_accepts: &[],
        }
    }

    pub fn mpeg2video() -> Self {
        Self {
            codec_name: "mpeg2video",
            options: Vec::new(),
            preferred_pixel_format_8bit: Some(PixelFormat::Yuv420p),
            preferred_pixel_format_10bit: None,
            preferred_surface: FrameSurface::System,
            also_accepts: &[],
        }
    }

    pub(crate) fn as_arg(&self) -> ArgVec {
        let mut args = args!["-vcodec", self.codec_name];
        args.extend(self.options.iter().cloned());
        args
    }
}

#[derive(Clone, PartialEq)]
pub enum VideoEncoder {
    Copy,
    Encode(VideoCodec),
}

impl VideoEncoder {
    pub(crate) fn preferred_surface(&self) -> FrameSurface {
        match self {
            VideoEncoder::Copy => FrameSurface::System,
            VideoEncoder::Encode(codec) => codec.preferred_surface,
        }
    }

    pub(crate) fn also_accepts(&self, bit_depth: u8) -> Vec<PixelFormat> {
        match self {
            VideoEncoder::Copy => Vec::new(),
            VideoEncoder::Encode(codec) => codec
                .also_accepts
                .iter()
                .copied()
                .filter(|pf| pf.bit_depth() == bit_depth)
                .collect(),
        }
    }

    pub(crate) fn preferred_pixel_format(&self, bit_depth: u8) -> Option<PixelFormat> {
        match (self, bit_depth) {
            (VideoEncoder::Copy, _) => None,
            (VideoEncoder::Encode(codec), 10) => codec.preferred_pixel_format_10bit,
            (VideoEncoder::Encode(codec), 8) => codec.preferred_pixel_format_8bit,
            (VideoEncoder::Encode(_), _) => None,
        }
    }

    pub(crate) fn as_arg(&self) -> ArgVec {
        match self {
            VideoEncoder::Copy => args!["-vcodec", "copy"],
            VideoEncoder::Encode(codec) => codec.as_arg(),
        }
    }
}
