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
    /// formats hwupload can copy into video memory, keeping the format
    pub(crate) upload_formats: HashSet<QsvFourCC>,
    /// (input, output) pairs vpp_qsv can convert between in video memory
    pub(crate) convert_pairs: HashSet<(QsvFourCC, QsvFourCC)>,
    pub(crate) vpp_filters: HashSet<QsvFourCC>,
    pub(crate) rotation_formats: HashSet<QsvFourCC>,
    /// (input, output) composite pairs. the patched vpp_qsv pads with a composite
    pub(crate) composite_pairs: HashSet<(QsvFourCC, QsvFourCC)>,
    pub(crate) runtime_api: Option<(u16, u16)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct QsvFourCC(pub(crate) u32);

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

    pub fn can_upload(&self, pixel_format: &PixelFormat) -> bool {
        Self::fourcc(pixel_format).is_some_and(|c| self.upload_formats.contains(&c))
    }

    pub fn can_convert(&self, input: &PixelFormat, output: &PixelFormat) -> bool {
        Self::pair(input, output).is_some_and(|pair| self.convert_pairs.contains(&pair))
    }

    fn pair(input: &PixelFormat, output: &PixelFormat) -> Option<(QsvFourCC, QsvFourCC)> {
        Self::fourcc(input).zip(Self::fourcc(output))
    }

    fn fourcc(pixel_format: &PixelFormat) -> Option<QsvFourCC> {
        let fourcc = match pixel_format {
            PixelFormat::Nv12 | PixelFormat::Yuv420p => Some(MFX_FOURCC_NV12),
            PixelFormat::P010le | PixelFormat::Yuv420p10le => Some(MFX_FOURCC_P010),
            PixelFormat::Bgra => Some(MFX_FOURCC_RGB4),
            _ => None,
        };

        fourcc.map(QsvFourCC)
    }

    pub fn can_rotate(&self, pixel_format: &PixelFormat) -> bool {
        self.runtime_api.is_some_and(|version| version >= (1, 17))
            && Self::fourcc(pixel_format).is_some_and(|c| self.rotation_formats.contains(&c))
    }

    pub fn can_pad(&self, input: &PixelFormat, output: &PixelFormat) -> bool {
        Self::pair(input, output).is_some_and(|pair| self.composite_pairs.contains(&pair))
    }

    // this is just a heuristic; something to tighten up if we encounter tonemapping failures
    pub fn can_tonemap(&self) -> bool {
        self.runtime_api.is_some_and(|(major, _)| major >= 2)
            && self
                .vpp_filters
                .contains(&QsvFourCC(MFX_EXTBUFF_VIDEO_SIGNAL_INFO_IN))
            && self
                .vpp_filters
                .contains(&QsvFourCC(MFX_EXTBUFF_VIDEO_SIGNAL_INFO_OUT))
            && self.can_convert(&PixelFormat::P010le, &PixelFormat::Nv12)
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
            upload_formats: formats.iter().map(|f| QsvFourCC(*f)).collect(),
            convert_pairs: formats
                .iter()
                .flat_map(|input| {
                    formats
                        .iter()
                        .map(|output| (QsvFourCC(*input), QsvFourCC(*output)))
                })
                .collect(),
            vpp_filters: filters.iter().map(|f| QsvFourCC(*f)).collect(),
            rotation_formats: HashSet::new(),
            composite_pairs: HashSet::new(),
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
