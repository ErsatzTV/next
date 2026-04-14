use std::collections::HashSet;

use crate::pipeline::{PixelFormat, VideoFormat};

#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
pub(crate) mod vpl;

#[cfg(not(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
)))]
pub(crate) mod stub;

#[derive(Debug, Clone)]
pub struct QsvCapabilities {
    pub(crate) supported_decoders: HashSet<(VideoFormat, u8)>, // (format, bit_depth)
    pub(crate) supported_encoders: HashSet<(VideoFormat, u8)>,
}

impl QsvCapabilities {
    pub fn can_decode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_decoders.contains(&(*format, bit_depth))
    }

    pub fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_encoders.contains(&(*format, bit_depth))
    }

    pub fn vpp_supports_format(&self, pixel_format: &PixelFormat) -> bool {
        // TODO: actual format probe
        matches!(pixel_format, PixelFormat::Nv12 | PixelFormat::P010le)
    }
}
