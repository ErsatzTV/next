use std::collections::HashSet;

use crate::pipeline::{PixelFormat, VideoFormat};

#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub(crate) mod probe;

#[cfg(not(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
pub(crate) mod stub;

#[derive(Debug, Clone)]
pub struct NvidiaCapabilities {
    pub(crate) supported_decoders: HashSet<(VideoFormat, u8)>,
    pub(crate) supported_encoders: HashSet<(VideoFormat, u8)>,
}

impl NvidiaCapabilities {
    pub fn can_decode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_decoders.contains(&(*format, bit_depth))
    }

    pub fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_encoders.contains(&(*format, bit_depth))
    }

    pub fn vpp_supports_format(&self, pixel_format: &PixelFormat) -> bool {
        matches!(pixel_format, PixelFormat::Nv12 | PixelFormat::P010le)
    }
}
