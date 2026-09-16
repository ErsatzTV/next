use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Formatter};

use libamf_sys::{AMF_SURFACE_BGRA, AMF_SURFACE_NV12, AMF_SURFACE_P010, amf_surface_format_name};
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

/// AMF_MAKE_FULL_VERSION(1, 4, 32, 0), the floor libavcodec/amfenc.c applies to P010
const MIN_RUNTIME_FOR_10BIT_ENCODE: (u16, u16, u16, u16) = (1, 4, 32, 0);

/// The converter's color management landed in AMF SDK 1.4.34 (driver 24.6.1). PQ to
/// bt709 conversion of decoder P010 surfaces is verified on 1.4.37 (RDNA2) and crashes
/// the 1.4.31 driver, see [`AmfCapabilities::vpp_accepts_input`].
const MIN_RUNTIME_FOR_VPP_TONEMAP: (u16, u16, u16, u16) = (1, 4, 34, 0);

/// ffmpeg's amf hwcontext tries the same backends in the same order, so this is also
/// where ffmpeg's frames will live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AmfDevice {
    Dx11,
    Dx9,
    Vulkan,
}

impl AmfDevice {
    pub fn name(&self) -> &'static str {
        match self {
            AmfDevice::Dx11 => "DX11",
            AmfDevice::Dx9 => "DX9",
            AmfDevice::Vulkan => "Vulkan",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct AmfSurfaceFormat(pub(crate) i32);

impl Debug for AmfSurfaceFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", amf_surface_format_name(self.0))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AmfEncoderCapability {
    pub bit_depths: Vec<u8>,
    pub b_frames: bool,
    pub max_profile: Option<i64>,
    pub max_level: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmfCapabilities {
    pub(crate) supported_decoders: HashMap<VideoFormat, Vec<u8>>,
    pub(crate) supported_encoders: HashMap<VideoFormat, AmfEncoderCapability>,
    pub(crate) vpp_input_formats: HashSet<AmfSurfaceFormat>,
    pub(crate) vpp_output_formats: HashSet<AmfSurfaceFormat>,
    pub(crate) runtime_version: Option<(u16, u16, u16, u16)>,
    pub(crate) device: Option<AmfDevice>,
}

impl AmfCapabilities {
    pub fn can_decode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_decoders
            .get(format)
            .is_some_and(|bit_depths| bit_depths.contains(&bit_depth))
    }

    /// ffmpeg's amf encoders refuse P010 on runtimes older than 1.4.32 (driver 23.30)
    /// whatever the hardware supports.
    pub fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        if bit_depth == 10 && !self.runtime_supports_10bit_encode() {
            return false;
        }
        self.supported_encoders
            .get(format)
            .is_some_and(|cap| cap.bit_depths.contains(&bit_depth))
    }

    fn runtime_supports_10bit_encode(&self) -> bool {
        self.runtime_version
            .is_some_and(|version| version >= MIN_RUNTIME_FOR_10BIT_ENCODE)
    }

    pub fn b_frames(&self, format: &VideoFormat) -> bool {
        self.supported_encoders
            .get(format)
            .is_some_and(|cap| cap.b_frames)
    }

    pub fn encoder(&self, format: &VideoFormat) -> Option<&AmfEncoderCapability> {
        self.supported_encoders.get(format)
    }

    /// vpp_amf both consumes and produces the format, so it must appear on both sides.
    /// Software formats map to the surface they become after hwupload, as in the QSV
    /// capabilities.
    pub fn vpp_supports_format(&self, pixel_format: &PixelFormat) -> bool {
        vpp_surface(pixel_format).is_some_and(|s| {
            self.vpp_input_formats.contains(&s) && self.vpp_output_formats.contains(&s)
        })
    }

    /// Whether the converter can consume surfaces in this format. Older runtimes (1.4.31
    /// on Polaris) decode hevc 10-bit to P010 but list no P010 converter input, and
    /// feeding those surfaces to vpp_amf crashes inside the driver instead of failing.
    pub fn vpp_accepts_input(&self, pixel_format: &PixelFormat) -> bool {
        vpp_surface(pixel_format).is_some_and(|s| self.vpp_input_formats.contains(&s))
    }

    /// Whether vpp_amf can convert decoder PQ surfaces to bt709 nv12. The converter
    /// caps never list P010 input, even on runtimes that handle it, so this checks the
    /// runtime version instead. Only the tonemap works: the same converter still
    /// mangles plain P010 to NV12 and PQ to P010.
    pub fn can_tonemap(&self) -> bool {
        self.runtime_version
            .is_some_and(|version| version >= MIN_RUNTIME_FOR_VPP_TONEMAP)
    }

    pub fn vpp_input_formats(&self) -> Vec<String> {
        sorted_names(&self.vpp_input_formats)
    }

    pub fn vpp_output_formats(&self) -> Vec<String> {
        sorted_names(&self.vpp_output_formats)
    }

    pub fn runtime_version(&self) -> Option<(u16, u16, u16, u16)> {
        self.runtime_version
    }

    pub fn device(&self) -> Option<AmfDevice> {
        self.device
    }

    pub fn count(&self) -> usize {
        self.supported_decoders.len() + self.supported_encoders.len()
    }
}

/// The AMF surface a software pixel format becomes after hwupload, or is decoded to.
fn vpp_surface(pixel_format: &PixelFormat) -> Option<AmfSurfaceFormat> {
    let surface = match pixel_format {
        PixelFormat::Nv12 | PixelFormat::Yuv420p => AMF_SURFACE_NV12,
        PixelFormat::P010le | PixelFormat::Yuv420p10le => AMF_SURFACE_P010,
        PixelFormat::Bgra => AMF_SURFACE_BGRA,
        _ => return None,
    };
    Some(AmfSurfaceFormat(surface))
}

fn sorted_names(formats: &HashSet<AmfSurfaceFormat>) -> Vec<String> {
    let mut codes: Vec<i32> = formats.iter().map(|f| f.0).collect();
    codes.sort_unstable();
    codes.into_iter().map(amf_surface_format_name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(input: &[i32], output: &[i32]) -> AmfCapabilities {
        caps_with_runtime(Some((1, 4, 35, 0)), input, output)
    }

    fn caps_with_runtime(
        runtime_version: Option<(u16, u16, u16, u16)>,
        input: &[i32],
        output: &[i32],
    ) -> AmfCapabilities {
        let mut supported_encoders = HashMap::new();
        supported_encoders.insert(
            VideoFormat::H264,
            AmfEncoderCapability {
                bit_depths: vec![8],
                b_frames: false,
                max_profile: Some(100),
                max_level: Some(52),
            },
        );
        supported_encoders.insert(
            VideoFormat::Hevc,
            AmfEncoderCapability {
                bit_depths: vec![8, 10],
                b_frames: true,
                max_profile: Some(2),
                max_level: Some(186),
            },
        );

        let mut supported_decoders = HashMap::new();
        supported_decoders.insert(VideoFormat::H264, vec![8]);
        supported_decoders.insert(VideoFormat::Hevc, vec![8, 10]);

        AmfCapabilities {
            supported_decoders,
            supported_encoders,
            vpp_input_formats: input.iter().map(|f| AmfSurfaceFormat(*f)).collect(),
            vpp_output_formats: output.iter().map(|f| AmfSurfaceFormat(*f)).collect(),
            runtime_version,
            device: Some(AmfDevice::Dx11),
        }
    }

    #[test]
    fn ten_bit_encode_requires_runtime_1_4_32() {
        assert!(
            !caps_with_runtime(Some((1, 4, 31, 0)), &[], &[]).can_encode(&VideoFormat::Hevc, 10)
        );
        assert!(caps_with_runtime(Some((1, 4, 31, 0)), &[], &[]).can_encode(&VideoFormat::Hevc, 8));
        assert!(
            caps_with_runtime(Some((1, 4, 32, 0)), &[], &[]).can_encode(&VideoFormat::Hevc, 10)
        );
        assert!(caps_with_runtime(Some((1, 5, 0, 0)), &[], &[]).can_encode(&VideoFormat::Hevc, 10));
        assert!(!caps_with_runtime(None, &[], &[]).can_encode(&VideoFormat::Hevc, 10));
    }

    #[test]
    fn tonemap_requires_runtime_1_4_34() {
        assert!(!caps_with_runtime(Some((1, 4, 31, 0)), &[], &[]).can_tonemap());
        assert!(!caps_with_runtime(Some((1, 4, 33, 0)), &[], &[]).can_tonemap());
        assert!(caps_with_runtime(Some((1, 4, 34, 0)), &[], &[]).can_tonemap());
        assert!(caps_with_runtime(Some((1, 5, 0, 0)), &[], &[]).can_tonemap());
        assert!(!caps_with_runtime(None, &[], &[]).can_tonemap());
    }

    #[test]
    fn decode_and_encode_follow_probed_bit_depths() {
        let caps = caps(&[], &[]);
        assert!(caps.can_decode(&VideoFormat::H264, 8));
        assert!(!caps.can_decode(&VideoFormat::H264, 10));
        assert!(caps.can_decode(&VideoFormat::Hevc, 10));
        assert!(!caps.can_decode(&VideoFormat::Av1, 8));

        assert!(caps.can_encode(&VideoFormat::Hevc, 10));
        assert!(!caps.can_encode(&VideoFormat::H264, 10));
        assert!(!caps.can_encode(&VideoFormat::Av1, 8));
        assert!(caps.b_frames(&VideoFormat::Hevc));
        assert!(!caps.b_frames(&VideoFormat::H264));
        assert!(!caps.b_frames(&VideoFormat::Vp9));
    }

    #[test]
    fn vpp_requires_format_on_both_sides() {
        let both = caps(
            &[AMF_SURFACE_NV12, AMF_SURFACE_P010, AMF_SURFACE_BGRA],
            &[AMF_SURFACE_NV12, AMF_SURFACE_P010, AMF_SURFACE_BGRA],
        );
        assert!(both.vpp_supports_format(&PixelFormat::Nv12));
        assert!(both.vpp_supports_format(&PixelFormat::Yuv420p));
        assert!(both.vpp_supports_format(&PixelFormat::P010le));
        assert!(both.vpp_supports_format(&PixelFormat::Yuv420p10le));
        assert!(both.vpp_supports_format(&PixelFormat::Bgra));
        assert!(!both.vpp_supports_format(&PixelFormat::Nv15));

        let input_only = caps(&[AMF_SURFACE_NV12, AMF_SURFACE_P010], &[AMF_SURFACE_NV12]);
        assert!(input_only.vpp_supports_format(&PixelFormat::Nv12));
        assert!(!input_only.vpp_supports_format(&PixelFormat::P010le));
        assert!(input_only.vpp_accepts_input(&PixelFormat::P010le));
        assert!(input_only.vpp_accepts_input(&PixelFormat::Yuv420p10le));
        assert!(!input_only.vpp_accepts_input(&PixelFormat::Bgra));
        assert!(!input_only.vpp_accepts_input(&PixelFormat::Nv15));
    }

    #[test]
    fn format_names_are_sorted_by_amf_code() {
        let caps = caps(&[AMF_SURFACE_P010, AMF_SURFACE_NV12], &[]);
        assert_eq!(caps.vpp_input_formats(), vec!["NV12", "P010"]);
        assert!(caps.vpp_output_formats().is_empty());
    }
}
