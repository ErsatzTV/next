use std::collections::HashSet;

use libmpp_sys::probe::*;

use crate::capabilities::rkmpp::RkmppCapabilities;
use crate::error::FFPipelineError;
use crate::pipeline::VideoFormat;

/// (MppCodingType, VideoFormat, supports_10bit_decode)
const DECODER_CODECS: &[(i32, VideoFormat, bool)] = &[
    (MPP_VIDEO_CodingAVC, VideoFormat::H264, false),
    (MPP_VIDEO_CodingHEVC, VideoFormat::Hevc, true),
    (MPP_VIDEO_CodingVP8, VideoFormat::Vp8, false),
    (MPP_VIDEO_CodingVP9, VideoFormat::Vp9, false),
];

/// ffmpeg only supports 8-bit encoding with rkmpp
const ENCODER_CODECS: &[(i32, VideoFormat)] = &[
    (MPP_VIDEO_CodingAVC, VideoFormat::H264),
    (MPP_VIDEO_CodingHEVC, VideoFormat::Hevc),
];

impl RkmppCapabilities {
    pub fn probe() -> Result<RkmppCapabilities, FFPipelineError> {
        let lib = MppLib::load().map_err(|e| {
            FFPipelineError::RkmppCapabilitiesError(format!(
                "failed to load librockchip_mpp.so.1: {e}"
            ))
        })?;

        let mut supported_decoders = HashSet::new();
        let mut supported_encoders = HashSet::new();

        for &(coding, format, supports_10bit) in DECODER_CODECS {
            if lib.is_supported(MPP_CTX_DEC, coding) {
                supported_decoders.insert((format, 8));
                if supports_10bit {
                    supported_decoders.insert((format, 10));
                }
            }
        }

        for &(coding, format) in ENCODER_CODECS {
            if lib.is_supported(MPP_CTX_ENC, coding) {
                supported_encoders.insert((format, 8));
            }
        }

        Ok(RkmppCapabilities {
            supported_decoders,
            supported_encoders,
        })
    }
}
