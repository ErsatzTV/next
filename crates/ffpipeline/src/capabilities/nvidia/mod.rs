use std::collections::HashMap;

use serde::Serialize;

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

#[derive(Debug, Clone, Serialize)]
pub struct EncoderCapability {
    pub bit_depths: Vec<u8>,
    pub b_frame_ref_mode: bool,
    pub max_b_frames: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NvidiaCapabilities {
    pub(crate) supported_decoders: HashMap<VideoFormat, Vec<u8>>,
    pub(crate) supported_encoders: HashMap<VideoFormat, EncoderCapability>,
    pub(crate) device_uuid: Option<[u8; 16]>,
}

impl NvidiaCapabilities {
    pub fn can_decode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_decoders
            .get(format)
            .is_some_and(|bit_depths| bit_depths.contains(&bit_depth))
    }

    pub fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_encoders
            .get(format)
            .is_some_and(|cap| cap.bit_depths.contains(&bit_depth))
    }

    pub fn count(&self) -> usize {
        self.supported_decoders.len() + self.supported_encoders.len()
    }

    pub fn b_frame_ref_mode(&self, format: &VideoFormat) -> bool {
        self.supported_encoders
            .get(format)
            .is_some_and(|cap| cap.b_frame_ref_mode)
    }

    pub fn max_b_frames(&self, format: &VideoFormat) -> u32 {
        self.supported_encoders
            .get(format)
            .map_or(0, |cap| cap.max_b_frames)
    }

    pub fn vpp_supports_format(&self, pixel_format: &PixelFormat) -> bool {
        matches!(pixel_format, PixelFormat::Nv12 | PixelFormat::P010)
    }

    pub fn device_uuid(&self) -> Option<[u8; 16]> {
        self.device_uuid
    }
}
