use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;

use crate::frame_size::FrameSize;
use crate::pipeline::VideoFormat;

#[cfg(target_os = "macos")]
pub(crate) mod probe;

#[cfg(not(target_os = "macos"))]
pub(crate) mod stub;

#[derive(Debug, Clone, Default, Serialize)]
pub struct VideoToolboxCapabilities {
    pub(crate) supported_decoders: HashSet<(VideoFormat, u8)>,
    pub(crate) supported_encoders: HashSet<(VideoFormat, u8)>,
    /// `None` skips the native size check (non-macOS and unit tests)
    #[serde(skip)]
    pub(crate) encode_sizes: Option<EncodeSizeCache>,
}

/// Each native size check takes 30-50ms, and the pipeline checks every item.
#[derive(Debug, Clone, Default)]
pub(crate) struct EncodeSizeCache(Arc<Mutex<HashMap<(VideoFormat, FrameSize), bool>>>);

impl VideoToolboxCapabilities {
    pub fn can_decode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_decoders.contains(&(*format, bit_depth))
    }

    pub fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.supported_encoders.contains(&(*format, bit_depth))
    }

    /// Some hardware encoders reject small frames, and ffmpeg won't fall back to the
    /// VideoToolbox software encoder without `-allow_sw`.
    pub fn can_encode_size(&self, format: &VideoFormat, size: FrameSize) -> bool {
        let Some(EncodeSizeCache(cache)) = &self.encode_sizes else {
            return true;
        };

        *cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry((*format, size))
            .or_insert_with(|| Self::probe_encode_size(format, size))
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

        let mut supported_encoders = HashSet::new();
        supported_encoders.insert((VideoFormat::H264, 8));
        supported_encoders.insert((VideoFormat::Hevc, 8));
        supported_encoders.insert((VideoFormat::Hevc, 10));

        VideoToolboxCapabilities {
            supported_decoders,
            supported_encoders,
            ..Default::default()
        }
    }

    #[test]
    fn can_decode_supported_codec() {
        let caps = sample_capabilities();
        assert!(caps.can_decode(&VideoFormat::H264, 8));
        assert!(caps.can_decode(&VideoFormat::Hevc, 8));
        assert!(caps.can_decode(&VideoFormat::Hevc, 10));
    }

    #[test]
    fn cannot_decode_unsupported_codec() {
        let caps = sample_capabilities();
        assert!(!caps.can_decode(&VideoFormat::H264, 10));
        assert!(!caps.can_decode(&VideoFormat::Av1, 8));
        assert!(!caps.can_decode(&VideoFormat::Vp9, 8));
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
        let caps = VideoToolboxCapabilities::default();
        assert!(!caps.can_decode(&VideoFormat::H264, 8));
        assert!(!caps.can_encode(&VideoFormat::Hevc, 8));
    }

    #[test]
    fn can_encode_any_size_without_native_check() {
        let caps = sample_capabilities();
        let size = FrameSize {
            width: 200,
            height: 200,
        };
        assert!(caps.can_encode_size(&VideoFormat::H264, size));
    }

    #[test]
    fn can_encode_size_uses_cached_result() {
        let size = FrameSize {
            width: 200,
            height: 200,
        };
        let cache = EncodeSizeCache::default();
        cache
            .0
            .lock()
            .unwrap()
            .insert((VideoFormat::H264, size), false);
        let caps = VideoToolboxCapabilities {
            encode_sizes: Some(cache),
            ..sample_capabilities()
        };
        assert!(!caps.can_encode_size(&VideoFormat::H264, size));
    }
}
