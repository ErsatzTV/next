use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Formatter};

use libvpl_sys::{MFX_FOURCC_NV12, MFX_FOURCC_P010};
use serde::Serialize;

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

#[derive(Debug, Clone, Serialize)]
pub struct QsvCapabilities {
    pub(crate) supported_decoders: HashMap<VideoFormat, Vec<u8>>,
    pub(crate) supported_encoders: HashMap<VideoFormat, Vec<u8>>,
    pub(crate) vpp_pixel_formats: HashSet<QsvPixelFormat>,
}

#[derive(Clone, PartialEq, Eq, Hash, Serialize)]
pub struct QsvPixelFormat(u32);

impl Debug for QsvPixelFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", String::from_utf8_lossy(&self.0.to_ne_bytes()))
    }
}

impl QsvCapabilities {
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

    pub fn vpp_supports_format(&self, pixel_format: &PixelFormat) -> bool {
        let fourcc = match pixel_format {
            PixelFormat::Nv12 | PixelFormat::Yuv420p => Some(MFX_FOURCC_NV12),
            PixelFormat::P010le | PixelFormat::Yuv420p10le => Some(MFX_FOURCC_P010),
            _ => None,
        };

        fourcc.is_some_and(|c| self.vpp_pixel_formats.contains(&QsvPixelFormat(c)))
    }
}
