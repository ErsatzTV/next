use std::collections::HashSet;

use libvpl_sys::*;

use crate::capabilities::qsv::QsvCapabilities;
use crate::error::FFPipelineError;
use crate::pipeline::VideoFormat;

// byte offsets of dec and enc inside mfxImplDescription.
const IMPL_DESC_DEC_OFFSET: usize = 472;
const IMPL_DESC_ENC_OFFSET: usize = 504;

impl QsvCapabilities {
    pub fn probe() -> Result<QsvCapabilities, FFPipelineError> {
        let mut supported_decoders = HashSet::new();
        let mut supported_encoders = HashSet::new();

        unsafe {
            let loader = MFXLoad();
            if loader.is_null() {
                return Err(FFPipelineError::QsvCapabilitiesError(
                    "MFXLoad failed".into(),
                ));
            }

            // filter for hardware implementations only
            let config = MFXCreateConfig(loader);
            if !config.is_null() {
                let variant = mfxVariant {
                    Version: 0,
                    Type: MFX_VARIANT_TYPE_U32,
                    Data: mfxVariantValue {
                        U32: MFX_IMPL_TYPE_HARDWARE,
                    },
                };
                let name = b"mfxImplDescription.Impl\0";
                MFXSetConfigFilterProperty(config, name.as_ptr(), variant);
            }

            // request the implementation description struct from the first matching impl
            let mut hdl: mfxHDL = std::ptr::null_mut();
            let status =
                MFXEnumImplementations(loader, 0, MFX_IMPLCAPS_IMPLDESCSTRUCTURE, &mut hdl);

            if status == MFX_ERR_NONE && !hdl.is_null() {
                // dec and enc are embedded at fixed offsets inside mfxImplDescription
                // we access them directly by byte offset to avoid defining the full 648-byte struct
                let base = hdl as *const u8;
                let dec = &*(base.add(IMPL_DESC_DEC_OFFSET) as *const mfxDecoderDescription);
                let enc = &*(base.add(IMPL_DESC_ENC_OFFSET) as *const mfxEncoderDescription);

                for format in [VideoFormat::H264, VideoFormat::Hevc] {
                    let codec_id = match format {
                        VideoFormat::H264 => MFX_CODEC_AVC,
                        VideoFormat::Hevc => MFX_CODEC_HEVC,
                    };

                    if decoder_has_codec(dec, codec_id) {
                        supported_decoders.insert((format, 8u8));
                        if decoder_has_10bit_profile(dec, codec_id) {
                            supported_decoders.insert((format, 10u8));
                        }
                    }

                    let enc_profiles = encoder_profiles(enc, codec_id);
                    if !enc_profiles.is_empty() {
                        supported_encoders.insert((format, 8u8));
                        if encoder_supports_10bit(&enc_profiles, codec_id) {
                            supported_encoders.insert((format, 10u8));
                        }
                    }
                }

                MFXDispReleaseImplDescription(loader, hdl);
            }

            MFXUnload(loader);
        }

        Ok(QsvCapabilities {
            supported_decoders,
            supported_encoders,
        })
    }
}

/// Returns true if the decoder description lists the given codec.
unsafe fn decoder_has_codec(dec: &mfxDecoderDescription, codec_id: u32) -> bool {
    if dec.NumCodecs == 0 || dec.Codecs.is_null() {
        return false;
    }
    for i in 0..dec.NumCodecs as usize {
        let entry = unsafe { &*dec.Codecs.add(i) };
        if entry.CodecID == codec_id {
            return true;
        }
    }
    false
}

/// Returns true if the decoder description lists a 10-bit profile for the codec.
unsafe fn decoder_has_10bit_profile(dec: &mfxDecoderDescription, codec_id: u32) -> bool {
    if dec.NumCodecs == 0 || dec.Codecs.is_null() {
        return false;
    }
    for i in 0..dec.NumCodecs as usize {
        let entry = unsafe { &*dec.Codecs.add(i) };
        if entry.CodecID != codec_id {
            continue;
        }
        if entry.NumProfiles == 0 || entry.Profiles.is_null() {
            return false;
        }
        for j in 0..entry.NumProfiles as usize {
            let profile = unsafe { &*entry.Profiles.add(j) };
            if is_10bit_profile(codec_id, profile.Profile) {
                return true;
            }
        }
        break;
    }
    false
}

/// Collect all encoder profile IDs for the given codec.
unsafe fn encoder_profiles(enc: &mfxEncoderDescription, codec_id: u32) -> Vec<u32> {
    let mut profiles = Vec::new();
    if enc.NumCodecs == 0 || enc.Codecs.is_null() {
        return profiles;
    }
    for i in 0..enc.NumCodecs as usize {
        let entry = unsafe { &*enc.Codecs.add(i) };
        if entry.CodecID != codec_id {
            continue;
        }
        if !entry.Profiles.is_null() {
            for j in 0..entry.NumProfiles as usize {
                let profile = unsafe { &*entry.Profiles.add(j) };
                profiles.push(profile.Profile);
            }
        }
        break;
    }
    profiles
}

/// Returns true if any profile in the list indicates 10-bit encoding support.
fn encoder_supports_10bit(profiles: &[u32], codec_id: u32) -> bool {
    profiles.iter().any(|&p| is_10bit_profile(codec_id, p))
}

/// Returns true if the given profile ID implies 10-bit support for this codec.
fn is_10bit_profile(codec_id: u32, profile: u32) -> bool {
    match codec_id {
        id if id == MFX_CODEC_AVC => profile == MFX_PROFILE_AVC_HIGH10,
        id if id == MFX_CODEC_HEVC => profile == MFX_PROFILE_HEVC_MAIN10,
        _ => false,
    }
}
