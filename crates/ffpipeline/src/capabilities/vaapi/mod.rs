use std::collections::{BTreeMap, HashMap, HashSet};

use libva_sys::*;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

use crate::pipeline::{PixelFormat, VideoFormat};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod stub;

type FourCC = u32;

#[derive(Debug, Clone, Serialize)]
pub struct VaapiCapabilities {
    pub(crate) vendor: String,
    #[serde(serialize_with = "serialize_profile_entrypoint_set")]
    pub(crate) supported: HashSet<(i32, i32)>,
    /// FourCC of supported pixel formats.
    #[serde(serialize_with = "serialize_fourcc_set")]
    pub(crate) vpp_pixel_formats: HashSet<FourCC>,
    /// FourCC of supported HDR->SDR tonemap formats.
    #[serde(serialize_with = "serialize_fourcc_set")]
    pub(crate) can_hdr_to_sdr_tonemap: HashSet<FourCC>,
    /// FourCC of supported HDR->HDR tonemap formats.   
    #[serde(serialize_with = "serialize_fourcc_set")]
    pub(crate) can_hdr_to_hdr_tonemap: HashSet<FourCC>,
    pub(crate) can_overlay: bool,
    /// Bitmask of VA_RC_* per (profile, entrypoint). Absent = unknown / not queried.
    #[serde(serialize_with = "serialize_rate_control")]
    pub(crate) rate_control: HashMap<(i32, i32), u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateControlMode {
    Cqp,
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

    fn decode_profile_for(codec: &str, profile: &str, bit_depth: u8) -> Option<VAProfile> {
        let codec = codec.to_lowercase();
        let profile = profile.to_lowercase();

        // each entry pairs a VA profile with the maximum bit depth that profile
        // accepts. a bit depth above the max means the (profile, bit_depth)
        // combination is not representable, so we treat it as unsupported.
        let (va_profile, max_bit_depth) = match (codec.as_str(), profile.as_str()) {
            ("h264", "main" | "77") => (VA_PROFILE_H264_MAIN, 8),
            ("h264", "high" | "100") => (VA_PROFILE_H264_HIGH, 8),
            ("h264", "high 10" | "110") => (VA_PROFILE_H264_HIGH10, 10),
            ("h264", "baseline constrained" | "constrained baseline" | "578") => {
                (VA_PROFILE_H264_CONSTRAINED_BASELINE, 8)
            }
            ("mpeg2video", "main" | "4") => (VA_PROFILE_MPEG2_MAIN, 8),
            ("mpeg2video", "simple" | "5") => (VA_PROFILE_MPEG2_SIMPLE, 8),
            ("vc1", "simple" | "0") => (VA_PROFILE_VC1_SIMPLE, 8),
            ("vc1", "main" | "1") => (VA_PROFILE_VC1_MAIN, 8),
            ("vc1", "advanced" | "3") => (VA_PROFILE_VC1_ADVANCED, 8),
            ("hevc", "main" | "1") => (VA_PROFILE_HEVC_MAIN, 8),
            ("hevc", "main 10" | "2") => (VA_PROFILE_HEVC_MAIN10, 10),
            ("vp8", "0") => (VA_PROFILE_VP8_VERSION0_3, 8),
            ("vp9", "profile 0" | "0") => (VA_PROFILE_VP9_PROFILE0, 8),
            ("vp9", "profile 1" | "1") => (VA_PROFILE_VP9_PROFILE1, 8),
            ("vp9", "profile 2" | "2") => (VA_PROFILE_VP9_PROFILE2, 10),
            ("vp9", "profile 3" | "3") => (VA_PROFILE_VP9_PROFILE3, 10),
            ("av1", "main" | "0") => (VA_PROFILE_AV1_PROFILE0, 10),
            _ => return None,
        };

        if bit_depth > max_bit_depth {
            return None;
        }

        Some(va_profile)
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
            PixelFormat::Bgra => Some(VA_FOURCC_BGRA),
            _ => None,
        }
    }

    pub fn can_overlay(&self) -> bool {
        self.can_overlay
    }

    /// Returns Some(Cqp) when the only available RC mode is CQP and we must force it.
    /// Returns None when VBR/CBR is available (driver default is fine) or nothing is known.
    pub fn rate_control_mode_for(
        &self,
        format: &VideoFormat,
        bit_depth: u8,
    ) -> Option<RateControlMode> {
        let profile = Self::encode_profile_for(format, bit_depth)?;
        // try ENC_SLICE then ENC_SLICE_LP
        for ep in [VA_ENTRYPOINT_ENC_SLICE, VA_ENTRYPOINT_ENC_SLICE_LP] {
            if let Some(&mask) = self.rate_control.get(&(profile, ep)) {
                if mask & (VA_RC_VBR | VA_RC_CBR) != 0 {
                    return None; // default RC is fine
                }
                if mask & VA_RC_CQP != 0 {
                    return Some(RateControlMode::Cqp);
                }
            }
        }

        None
    }
}

fn fourcc_str(fourcc: u32) -> String {
    String::from_utf8_lossy(&fourcc.to_le_bytes())
        .trim_end_matches('\0')
        .to_owned()
}

fn profile_name(p: i32) -> String {
    match p {
        VA_PROFILE_NONE => "None".into(),
        VA_PROFILE_MPEG2_SIMPLE => "MPEG2Simple".into(),
        VA_PROFILE_MPEG2_MAIN => "MPEG2Main".into(),
        VA_PROFILE_MPEG4_SIMPLE => "MPEG4Simple".into(),
        VA_PROFILE_MPEG4_ADVANCED_SIMPLE => "MPEG4AdvancedSimple".into(),
        VA_PROFILE_MPEG4_MAIN => "MPEG4Main".into(),
        VA_PROFILE_H264_MAIN => "H264Main".into(),
        VA_PROFILE_H264_HIGH => "H264High".into(),
        VA_PROFILE_VC1_SIMPLE => "VC1Simple".into(),
        VA_PROFILE_VC1_MAIN => "VC1Main".into(),
        VA_PROFILE_VC1_ADVANCED => "VC1Advanced".into(),
        VA_PROFILE_H264_CONSTRAINED_BASELINE => "H264ConstrainedBaseline".into(),
        VA_PROFILE_VP8_VERSION0_3 => "VP8Version0_3".into(),
        VA_PROFILE_HEVC_MAIN => "HEVCMain".into(),
        VA_PROFILE_HEVC_MAIN10 => "HEVCMain10".into(),
        VA_PROFILE_VP9_PROFILE0 => "VP9Profile0".into(),
        VA_PROFILE_VP9_PROFILE1 => "VP9Profile1".into(),
        VA_PROFILE_VP9_PROFILE2 => "VP9Profile2".into(),
        VA_PROFILE_VP9_PROFILE3 => "VP9Profile3".into(),
        VA_PROFILE_AV1_PROFILE0 => "AV1Profile0".into(),
        VA_PROFILE_AV1_PROFILE1 => "AV1Profile1".into(),
        VA_PROFILE_H264_HIGH10 => "H264High10".into(),
        _ => format!("Profile({p})"),
    }
}

fn entrypoint_name(e: i32) -> String {
    match e {
        VA_ENTRYPOINT_VLD => "VLD".into(),
        VA_ENTRYPOINT_ENC_SLICE => "EncSlice".into(),
        VA_ENTRYPOINT_ENC_SLICE_LP => "EncSliceLP".into(),
        VA_ENTRYPOINT_VIDEO_PROC => "VideoProc".into(),
        _ => format!("Entrypoint({e})"),
    }
}

fn rc_modes(mask: u32) -> Vec<&'static str> {
    [(VA_RC_CQP, "CQP"), (VA_RC_CBR, "CBR"), (VA_RC_VBR, "VBR")]
        .iter()
        .filter_map(|(bit, name)| (mask & bit != 0).then_some(*name))
        .collect()
}

fn serialize_fourcc_set<S: Serializer>(set: &HashSet<FourCC>, s: S) -> Result<S::Ok, S::Error> {
    let mut v: Vec<String> = set.iter().copied().map(fourcc_str).collect();
    v.sort();
    v.serialize(s)
}

fn serialize_profile_entrypoint_set<S: Serializer>(
    set: &HashSet<(i32, i32)>,
    s: S,
) -> Result<S::Ok, S::Error> {
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for &(p, e) in set {
        grouped
            .entry(profile_name(p))
            .or_default()
            .push(entrypoint_name(e));
    }
    for v in grouped.values_mut() {
        v.sort();
        v.dedup();
    }
    grouped.serialize(s)
}

fn serialize_rate_control<S: Serializer>(
    map: &HashMap<(i32, i32), u32>,
    s: S,
) -> Result<S::Ok, S::Error> {
    let mut sorted: Vec<_> = map.iter().collect();
    sorted.sort_by_key(|((p, e), _)| (*p, *e));
    let mut m = s.serialize_map(Some(sorted.len()))?;
    for ((p, e), &mask) in sorted {
        let key = format!("{}/{}", profile_name(*p), entrypoint_name(*e));
        m.serialize_entry(&key, &rc_modes(mask))?;
    }
    m.end()
}
