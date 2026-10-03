use std::collections::HashSet;

use libvt_sys::probe::*;

use crate::capabilities::videotoolbox::{EncodeSizeCache, VideoToolboxCapabilities};
use crate::error::FFPipelineError;
use crate::frame_size::FrameSize;
use crate::pipeline::VideoFormat;

/// Codec types to probe, paired with their VideoFormat and whether 10-bit is
/// expected when hardware support is present.
const CODECS: &[(u32, VideoFormat, bool)] = &[
    (kCMVideoCodecType_H264, VideoFormat::H264, false),
    (kCMVideoCodecType_HEVC, VideoFormat::Hevc, true),
    (kCMVideoCodecType_VP9, VideoFormat::Vp9, true),
    (kCMVideoCodecType_AV1, VideoFormat::Av1, true),
];

impl VideoToolboxCapabilities {
    pub fn probe() -> Result<VideoToolboxCapabilities, FFPipelineError> {
        let mut supported_decoders = HashSet::new();
        let mut supported_encoders = HashSet::new();

        // Probe decoder support
        for &(codec_type, format, supports_10bit) in CODECS {
            if is_hardware_decode_supported(codec_type) {
                supported_decoders.insert((format, 8));
                if supports_10bit {
                    supported_decoders.insert((format, 10));
                }
            }
        }

        // Probe encoder support via VTCopyVideoEncoderList
        let hw_encoder_types = hardware_encoder_codec_types();
        for &(codec_type, format, supports_10bit) in CODECS {
            if hw_encoder_types.contains(&codec_type) {
                supported_encoders.insert((format, 8));
                if supports_10bit {
                    supported_encoders.insert((format, 10));
                }
            }
        }

        Ok(VideoToolboxCapabilities {
            supported_decoders,
            supported_encoders,
            encode_sizes: Some(EncodeSizeCache::default()),
        })
    }

    pub(crate) fn probe_encode_size(format: &VideoFormat, size: FrameSize) -> bool {
        CODECS
            .iter()
            .find(|(_, f, _)| f == format)
            .is_some_and(|&(codec_type, _, _)| {
                is_hardware_encode_supported(codec_type, size.width, size.height)
            })
    }
}
