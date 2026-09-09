use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Formatter};

use libvpl_sys::{
    MFX_EXTBUFF_VIDEO_SIGNAL_INFO_IN, MFX_EXTBUFF_VIDEO_SIGNAL_INFO_OUT, MFX_FOURCC_NV12,
    MFX_FOURCC_P010, MFX_FOURCC_RGB4,
};
use serde::Serialize;

use crate::pipeline::{PixelFormat, VideoFormat};

#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
pub(crate) mod legacy;

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
    pub(crate) vpp_pixel_formats: HashSet<QsvFourCC>,
    pub(crate) vpp_filters: HashSet<QsvFourCC>,
    pub(crate) runtime_api: Option<(u16, u16)>,
}

#[derive(Clone, PartialEq, Eq, Hash, Serialize)]
pub struct QsvFourCC(u32);

impl Debug for QsvFourCC {
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
            PixelFormat::Bgra => Some(MFX_FOURCC_RGB4),
            _ => None,
        };

        fourcc.is_some_and(|c| self.vpp_pixel_formats.contains(&QsvFourCC(c)))
    }

    // this is just a heuristic; and p010 support could mean input or output to any filter
    // something to tighten up if we encounter tonemapping failures
    pub fn can_tonemap(&self) -> bool {
        self.runtime_api.is_some_and(|(major, _)| major >= 2)
            && self
                .vpp_filters
                .contains(&QsvFourCC(MFX_EXTBUFF_VIDEO_SIGNAL_INFO_IN))
            && self
                .vpp_filters
                .contains(&QsvFourCC(MFX_EXTBUFF_VIDEO_SIGNAL_INFO_OUT))
            && self.vpp_supports_format(&PixelFormat::P010le)
    }

    pub fn runtime_api(&self) -> Option<(u16, u16)> {
        self.runtime_api
    }

    pub fn vpp_filters(&self) -> Vec<String> {
        let mut filters: Vec<String> = self
            .vpp_filters
            .iter()
            .map(|f| String::from_utf8_lossy(&f.0.to_ne_bytes()).into_owned())
            .collect();
        filters.sort();
        filters
    }

    pub fn count(&self) -> usize {
        self.supported_decoders.len() + self.supported_encoders.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(runtime_api: Option<(u16, u16)>, filters: &[u32], formats: &[u32]) -> QsvCapabilities {
        QsvCapabilities {
            supported_decoders: HashMap::new(),
            supported_encoders: HashMap::new(),
            vpp_pixel_formats: formats.iter().map(|f| QsvFourCC(*f)).collect(),
            vpp_filters: filters.iter().map(|f| QsvFourCC(*f)).collect(),
            runtime_api,
        }
    }

    const VSI: &[u32] = &[
        MFX_EXTBUFF_VIDEO_SIGNAL_INFO_IN,
        MFX_EXTBUFF_VIDEO_SIGNAL_INFO_OUT,
    ];

    #[test]
    fn can_tonemap_on_vpl_runtime_with_signal_info_and_p010() {
        assert!(caps(Some((2, 17)), VSI, &[MFX_FOURCC_NV12, MFX_FOURCC_P010]).can_tonemap());
    }

    #[test]
    fn cannot_tonemap_on_legacy_runtime() {
        assert!(!caps(Some((1, 35)), VSI, &[MFX_FOURCC_P010]).can_tonemap());
        assert!(!caps(Some((1, 35)), &[], &[MFX_FOURCC_P010]).can_tonemap());
        assert!(!caps(None, VSI, &[MFX_FOURCC_P010]).can_tonemap());
    }

    #[test]
    fn cannot_tonemap_without_both_signal_info_filters() {
        assert!(
            !caps(
                Some((2, 17)),
                &[MFX_EXTBUFF_VIDEO_SIGNAL_INFO_IN],
                &[MFX_FOURCC_P010]
            )
            .can_tonemap()
        );
        assert!(!caps(Some((2, 17)), &[], &[MFX_FOURCC_P010]).can_tonemap());
    }

    #[test]
    fn cannot_tonemap_without_p010_vpp_support() {
        assert!(!caps(Some((2, 17)), VSI, &[MFX_FOURCC_NV12]).can_tonemap());
    }
}
