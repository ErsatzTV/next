use std::collections::HashMap;

use serde::Serialize;

use crate::pipeline::VideoFormat;

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
pub struct VulkanCapabilities {
    pub(crate) device_index: u32,
    pub(crate) supported_decoders: HashMap<VideoFormat, Vec<u8>>,
    pub(crate) supported_encoders: HashMap<VideoFormat, Vec<u8>>,
}

impl VulkanCapabilities {
    pub fn can_decode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_decoders
            .get(format)
            .is_some_and(|bit_depths| bit_depths.contains(&bit_depth))
    }

    pub fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_encoders
            .get(format)
            .is_some_and(|bit_depths| bit_depths.contains(&bit_depth))
    }
}
