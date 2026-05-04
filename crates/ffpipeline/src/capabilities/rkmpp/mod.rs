use std::collections::HashSet;

use crate::pipeline::VideoFormat;

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
pub(crate) mod probe;

#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
pub(crate) mod stub;

#[derive(Debug, Clone)]
pub struct RkmppCapabilities {
    pub(crate) supported_decoders: HashSet<(VideoFormat, u8)>,
    pub(crate) supported_encoders: HashSet<(VideoFormat, u8)>,
}

impl RkmppCapabilities {
    pub fn can_decode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_decoders.contains(&(*format, bit_depth))
    }

    pub fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_encoders.contains(&(*format, bit_depth))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_capabilities() -> RkmppCapabilities {
        let mut supported_decoders = HashSet::new();
        supported_decoders.insert((VideoFormat::H264, 8));
        supported_decoders.insert((VideoFormat::Hevc, 8));
        supported_decoders.insert((VideoFormat::Hevc, 10));
        supported_decoders.insert((VideoFormat::Vp9, 8));
        supported_decoders.insert((VideoFormat::Vp9, 10));

        let mut supported_encoders = HashSet::new();
        supported_encoders.insert((VideoFormat::H264, 8));
        supported_encoders.insert((VideoFormat::Hevc, 8));

        RkmppCapabilities {
            supported_decoders,
            supported_encoders,
        }
    }

    #[test]
    fn can_decode_supported_codec() {
        let caps = sample_capabilities();
        assert!(caps.can_decode(&VideoFormat::H264, 8));
        assert!(caps.can_decode(&VideoFormat::Hevc, 10));
        assert!(caps.can_decode(&VideoFormat::Vp9, 10));
    }

    #[test]
    fn cannot_decode_unsupported_codec() {
        let caps = sample_capabilities();
        assert!(!caps.can_decode(&VideoFormat::H264, 10));
        assert!(!caps.can_decode(&VideoFormat::Av1, 8));
    }

    #[test]
    fn encoder_is_8bit_only() {
        let caps = sample_capabilities();
        assert!(caps.can_encode(&VideoFormat::H264, 8));
        assert!(caps.can_encode(&VideoFormat::Hevc, 8));
        assert!(!caps.can_encode(&VideoFormat::Hevc, 10));
        assert!(!caps.can_encode(&VideoFormat::Vp9, 8));
    }
}
