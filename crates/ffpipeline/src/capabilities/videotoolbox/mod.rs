use std::collections::HashSet;

use serde::Serialize;

use crate::pipeline::{PixelFormat, Sampling, VideoFormat};

/// `false` where the backend below is a stub.
pub const HAS_PROBE: bool = cfg!(target_os = "macos");

#[cfg(target_os = "macos")]
pub(crate) mod probe;

#[cfg(not(target_os = "macos"))]
pub(crate) mod stub;

#[derive(Debug, Clone, Serialize)]
pub struct VideoToolboxCapabilities {
    pub(crate) supported_decoders: HashSet<(VideoFormat, u8)>,
    pub(crate) supported_interlaced_decoders: HashSet<(VideoFormat, u8)>,
    pub(crate) supported_encoders: HashSet<(VideoFormat, u8)>,
}

impl VideoToolboxCapabilities {
    pub fn can_decode(&self, format: &VideoFormat, pixel_format: &PixelFormat) -> bool {
        if pixel_format.sampling() != Sampling::Yuv420 {
            return false;
        }

        self.supported_decoders
            .contains(&(*format, pixel_format.bit_depth()))
    }

    pub fn can_decode_interlaced(&self, format: &VideoFormat, pixel_format: &PixelFormat) -> bool {
        if pixel_format.sampling() != Sampling::Yuv420 {
            return false;
        }

        self.supported_interlaced_decoders
            .contains(&(*format, pixel_format.bit_depth()))
    }

    pub fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_encoders.contains(&(*format, bit_depth))
    }

    pub fn count(&self) -> usize {
        self.supported_decoders.len() + self.supported_encoders.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_capabilities() -> VideoToolboxCapabilities {
        let mut supported_decoders = HashSet::new();
        supported_decoders.insert((VideoFormat::H264, 8));
        supported_decoders.insert((VideoFormat::Hevc, 8));
        supported_decoders.insert((VideoFormat::Hevc, 10));

        let mut supported_interlaced_decoders = HashSet::new();
        supported_interlaced_decoders.insert((VideoFormat::Mpeg2Video, 8));

        let mut supported_encoders = HashSet::new();
        supported_encoders.insert((VideoFormat::H264, 8));
        supported_encoders.insert((VideoFormat::Hevc, 8));
        supported_encoders.insert((VideoFormat::Hevc, 10));

        VideoToolboxCapabilities {
            supported_decoders,
            supported_interlaced_decoders,
            supported_encoders,
        }
    }

    #[test]
    fn can_decode_supported_codec() {
        let caps = sample_capabilities();
        assert!(caps.can_decode(&VideoFormat::H264, &PixelFormat::Yuv420p));
        assert!(caps.can_decode(&VideoFormat::Hevc, &PixelFormat::Yuv420p));
        assert!(caps.can_decode(&VideoFormat::Hevc, &PixelFormat::Yuv420p10le));
    }

    #[test]
    fn cannot_decode_unsupported_codec() {
        let caps = sample_capabilities();
        assert!(!caps.can_decode(&VideoFormat::H264, &PixelFormat::Yuv420p10le));
        assert!(!caps.can_decode(&VideoFormat::Av1, &PixelFormat::Yuv420p));
        assert!(!caps.can_decode(&VideoFormat::Vp9, &PixelFormat::Yuv420p));
    }

    #[test]
    fn interlaced_decode_tracked_separately() {
        let caps = sample_capabilities();
        assert!(caps.can_decode(&VideoFormat::H264, &PixelFormat::Yuv420p));
        assert!(!caps.can_decode_interlaced(&VideoFormat::H264, &PixelFormat::Yuv420p));
        assert!(caps.can_decode_interlaced(&VideoFormat::Mpeg2Video, &PixelFormat::Yuv420p));
    }

    #[test]
    fn can_encode_supported_codec() {
        let caps = sample_capabilities();
        assert!(caps.can_encode(&VideoFormat::H264, 8));
        assert!(caps.can_encode(&VideoFormat::Hevc, 10));
    }

    #[test]
    fn cannot_encode_unsupported_codec() {
        let caps = sample_capabilities();
        assert!(!caps.can_encode(&VideoFormat::H264, 10));
        assert!(!caps.can_encode(&VideoFormat::Av1, 8));
    }

    #[test]
    fn empty_capabilities_deny_all() {
        let caps = VideoToolboxCapabilities {
            supported_decoders: HashSet::new(),
            supported_interlaced_decoders: HashSet::new(),
            supported_encoders: HashSet::new(),
        };
        assert!(!caps.can_decode(&VideoFormat::H264, &PixelFormat::Yuv420p));
        assert!(!caps.can_encode(&VideoFormat::Hevc, 8));
    }
}
