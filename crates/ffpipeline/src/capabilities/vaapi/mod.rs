use std::collections::HashSet;

use libva_sys::*;

use crate::pipeline::{PixelFormat, VideoFormat};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod stub;

type FourCC = u32;

#[derive(Debug, Clone)]
pub struct VaapiCapabilities {
    pub(crate) vendor: String,
    pub(crate) supported: HashSet<(i32, i32)>,
    /// FourCC of supported pixel formats.
    pub(crate) vpp_pixel_formats: HashSet<FourCC>,
    /// FourCC of supported HDR->SDR tonemap formats.
    pub(crate) can_hdr_to_sdr_tonemap: HashSet<FourCC>,
    /// FourCC of supported HDR->HDR tonemap formats.   
    pub(crate) can_hdr_to_hdr_tonemap: HashSet<FourCC>,
}

impl VaapiCapabilities {
    pub fn vpp_supports_format(&self, pixel_format: &PixelFormat) -> bool {
        self.as_fourcc(pixel_format)
            .is_some_and(|c| self.vpp_pixel_formats.contains(&c))
    }

    pub fn can_decode(&self, codec: &str, profile: &str, bit_depth: u8) -> bool {
        Self::decode_profile_for(codec, profile, bit_depth)
            .iter()
            .any(|p| self.supported.contains(&(*p, VA_ENTRYPOINT_VLD)))
    }

    pub fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        Self::encode_profile_for(format, bit_depth)
            .iter()
            .any(|p| self.supported.contains(&(*p, VA_ENTRYPOINT_ENC_SLICE)))
    }

    pub fn can_encode_low_power(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        Self::encode_profile_for(format, bit_depth)
            .iter()
            .any(|p| self.supported.contains(&(*p, VA_ENTRYPOINT_ENC_SLICE_LP)))
    }

    fn decode_profile_for(codec: &str, profile: &str, _bit_depth: u8) -> Option<VAProfile> {
        match (codec, profile) {
            ("h264", "main" | "77") => Some(VA_PROFILE_H264_MAIN),
            ("h264", "high" | "100" | "high 10" | "110") => Some(VA_PROFILE_H264_HIGH),
            ("h264", "baseline constrained" | "constrained baseline" | "578") => {
                Some(VA_PROFILE_H264_CONSTRAINED_BASELINE)
            }
            ("mpeg2video", "main" | "4") => Some(VA_PROFILE_MPEG2_MAIN),
            ("mpeg2video", "simple" | "5") => Some(VA_PROFILE_MPEG2_SIMPLE),
            ("vc1", "simple" | "0") => Some(VA_PROFILE_VC1_SIMPLE),
            ("vc1", "main" | "1") => Some(VA_PROFILE_VC1_MAIN),
            ("vc1", "advanced" | "3") => Some(VA_PROFILE_VC1_ADVANCED),
            ("hevc", "main" | "1") => Some(VA_PROFILE_HEVC_MAIN),
            ("hevc", "main 10" | "2") => Some(VA_PROFILE_HEVC_MAIN10),
            ("vp9", "profile 0" | "0") => Some(VA_PROFILE_VP9_PROFILE0),
            ("vp9", "profile 1" | "1") => Some(VA_PROFILE_VP9_PROFILE1),
            ("vp9", "profile 2" | "2") => Some(VA_PROFILE_VP9_PROFILE2),
            ("vp9", "profile 3" | "3") => Some(VA_PROFILE_VP9_PROFILE3),
            ("av1", "main" | "0") => Some(VA_PROFILE_AV1_PROFILE0),
            _ => None,
        }
    }

    fn encode_profile_for(format: &VideoFormat, bit_depth: u8) -> Option<VAProfile> {
        match (format, bit_depth) {
            (VideoFormat::H264, 8) => Some(VA_PROFILE_H264_MAIN),
            (VideoFormat::Hevc, 8) => Some(VA_PROFILE_HEVC_MAIN),
            (VideoFormat::Hevc, 10) => Some(VA_PROFILE_HEVC_MAIN10),
            _ => None,
        }
    }

    pub fn vendor(&self) -> &str {
        &self.vendor
    }

    pub fn count(&self) -> usize {
        self.supported.len()
    }

    pub fn can_hdr_to_hdr_tonemap(&self, pixel_format: &PixelFormat) -> bool {
        self.as_fourcc(pixel_format)
            .is_some_and(|cc| self.can_hdr_to_hdr_tonemap.contains(&cc))
    }

    pub fn can_hdr_to_sdr_tonemap(&self, pixel_format: &PixelFormat) -> bool {
        self.as_fourcc(pixel_format)
            .is_some_and(|cc| self.can_hdr_to_sdr_tonemap.contains(&cc))
    }

    fn as_fourcc(&self, pixel_format: &PixelFormat) -> Option<u32> {
        match pixel_format {
            PixelFormat::Nv12 | PixelFormat::Yuv420p => Some(VA_FOURCC_NV12),
            PixelFormat::P010le | PixelFormat::Yuv420p10le => Some(VA_FOURCC_P010),
            _ => None,
        }
    }
}
