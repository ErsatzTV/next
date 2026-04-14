use std::collections::HashSet;

use libvpl_sys::*;

use crate::capabilities::qsv::QsvCapabilities;
use crate::error::FFPipelineError;
use crate::pipeline::VideoFormat;

impl QsvCapabilities {
    pub fn probe() -> Result<QsvCapabilities, FFPipelineError> {
        let mut supported_decoders = HashSet::new();
        let mut supported_encoders = HashSet::new();

        unsafe {
            let loader = MFXLoad();
            if loader.is_null() {
                return Err(FFPipelineError::QsvCapabilitiesError(String::from(
                    "MFXLoad failed",
                )));
            }

            let config = MFXCreateConfig(loader);
            if !config.is_null() {
                let variant = mfxVariant {
                    Version: 0,
                    Type: MFX_VARIANT_VALUE_TYPE_U32,
                    Data: mfxVariantValue {
                        U32: MFX_IMPL_HARDWARE as u32,
                    },
                };

                let name = b"mfxImplDescription.Impl\0";
                MFXSetConfigFilterProperty(config, name.as_ptr(), variant);
            }

            let mut session: mfxSession = std::ptr::null_mut();
            if MFXCreateSession(loader, 0, &mut session) == MFX_ERR_NONE {
                for format in [VideoFormat::H264, VideoFormat::Hevc] {
                    for bit_depth in [8, 10] {
                        if Self::probe_decode(session, format, bit_depth) {
                            supported_decoders.insert((format, bit_depth));
                        }
                    }
                }

                for format in [VideoFormat::H264, VideoFormat::Hevc] {
                    for bit_depth in [8, 10] {
                        if Self::probe_encode(session, format, bit_depth) {
                            supported_encoders.insert((format, bit_depth));
                        }
                    }
                }

                MFXClose(session);
            }

            MFXUnload(loader);
        }

        Ok(QsvCapabilities {
            supported_decoders,
            supported_encoders,
        })
    }

    unsafe fn probe_decode(session: mfxSession, format: VideoFormat, bit_depth: u8) -> bool {
        unsafe {
            let mut param: mfxVideoParam = std::mem::zeroed();
            param.mfx.CodecId = match format {
                VideoFormat::H264 => MFX_CODEC_AVC,
                VideoFormat::Hevc => MFX_CODEC_HEVC,
            };
            param.mfx.FrameInfo.FourCC = if bit_depth == 10 {
                MFX_FOURCC_P010
            } else {
                MFX_FOURCC_NV12
            };
            param.mfx.FrameInfo.ChromaFormat = 1; // yuv420
            param.mfx.FrameInfo.Width = 1920;
            param.mfx.FrameInfo.Height = 1080;

            MFXVideoDECODE_Query(session, &mut param, &mut param) == MFX_ERR_NONE
        }
    }

    unsafe fn probe_encode(session: mfxSession, format: VideoFormat, bit_depth: u8) -> bool {
        unsafe {
            let mut param: mfxVideoParam = std::mem::zeroed();
            param.mfx.CodecId = match format {
                VideoFormat::H264 => MFX_CODEC_AVC,
                VideoFormat::Hevc => MFX_CODEC_HEVC,
            };
            param.mfx.FrameInfo.FourCC = if bit_depth == 10 {
                MFX_FOURCC_P010
            } else {
                MFX_FOURCC_NV12
            };
            param.mfx.FrameInfo.ChromaFormat = 1; // yuv420
            param.mfx.FrameInfo.Width = 1920;
            param.mfx.FrameInfo.Height = 1080;
            param.mfx.FrameInfo.FrameRateExtN = 30;
            param.mfx.FrameInfo.FrameRateExtD = 1;

            param.mfx.CodecProfile = 1; // base profile

            MFXVideoENCODE_Query(session, &mut param, &mut param) == MFX_ERR_NONE
        }
    }
}
