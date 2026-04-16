use std::collections::HashMap;
use std::ffi::c_void;

use libnvidia_sys::cuda::{CUDA_SUCCESS, CudaLib};
use libnvidia_sys::cuvid::{
    CUDA_VIDEO_CHROMA_FORMAT_420, CUDA_VIDEO_CODEC_AV1, CUDA_VIDEO_CODEC_H264,
    CUDA_VIDEO_CODEC_HEVC, CUDA_VIDEO_CODEC_MPEG2, CUDA_VIDEO_CODEC_VC1, CUDA_VIDEO_CODEC_VP8,
    CUDA_VIDEO_CODEC_VP9, CuvidDecodeCaps, CuvidLib,
};
use libnvidia_sys::nvenc::{
    NV_ENC_CAPS_PARAM_VER, NV_ENC_CAPS_SUPPORT_10_BIT_ENCODE, NV_ENC_CAPS_SUPPORT_BFRAME_REF_MODE,
    NV_ENC_CODEC_AV1_GUID, NV_ENC_CODEC_H264_GUID, NV_ENC_CODEC_HEVC_GUID, NV_ENC_DEVICE_TYPE_CUDA,
    NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER, NV_ENC_SUCCESS, NV_ENCODE_API_FUNCTION_LIST_VER,
    NVENCAPI_VERSION, NvEncApiFunctionList, NvEncCapsParam, NvEncGuid,
    NvEncOpenEncodeSessionExParams, NvencLib,
};

use crate::capabilities::nvidia::{EncoderCapability, NvidiaCapabilities};
use crate::error::FFPipelineError;
use crate::pipeline::VideoFormat;

impl NvidiaCapabilities {
    pub fn probe() -> Result<NvidiaCapabilities, FFPipelineError> {
        let mut supported_decoders = HashMap::new();
        let mut supported_encoders = HashMap::new();

        let cuda = CudaLib::load().map_err(|e| {
            FFPipelineError::NvidiaCapabilitiesError(format!("failed to load libcuda: {e}"))
        })?;

        unsafe {
            if (cuda.cu_init)(0) != CUDA_SUCCESS {
                return Err(FFPipelineError::NvidiaCapabilitiesError(
                    "cuInit failed".into(),
                ));
            }

            let mut device: i32 = 0;
            if (cuda.cu_device_get)(&mut device, 0) != CUDA_SUCCESS {
                return Err(FFPipelineError::NvidiaCapabilitiesError(
                    "cuDeviceGet failed".into(),
                ));
            }

            let mut ctx: *mut c_void = std::ptr::null_mut();
            if (cuda.cu_ctx_create)(&mut ctx, 0, device) != CUDA_SUCCESS {
                return Err(FFPipelineError::NvidiaCapabilitiesError(
                    "cuCtxCreate failed".into(),
                ));
            }

            if let Ok(cuvid) = CuvidLib::load() {
                probe_decode(&cuvid, &mut supported_decoders);
            } else {
                log::warn!("libnvcuvid not available; skipping decode capability probe");
            }

            if let Ok(nvenc) = NvencLib::load() {
                probe_encode(&nvenc, ctx, &mut supported_encoders);
            } else {
                log::warn!("libnvidia-encode not available; skipping encode capability probe");
            }

            (cuda.cu_ctx_destroy)(ctx);
        }

        Ok(NvidiaCapabilities {
            supported_decoders,
            supported_encoders,
        })
    }
}

unsafe fn probe_decode(cuvid: &CuvidLib, supported: &mut HashMap<VideoFormat, Vec<u8>>) {
    unsafe {
        const CODECS: &[(VideoFormat, u32)] = &[
            (VideoFormat::H264, CUDA_VIDEO_CODEC_H264),
            (VideoFormat::Hevc, CUDA_VIDEO_CODEC_HEVC),
            (VideoFormat::Mpeg2Video, CUDA_VIDEO_CODEC_MPEG2),
            (VideoFormat::Vc1, CUDA_VIDEO_CODEC_VC1),
            (VideoFormat::Vp8, CUDA_VIDEO_CODEC_VP8),
            (VideoFormat::Vp9, CUDA_VIDEO_CODEC_VP9),
            (VideoFormat::Av1, CUDA_VIDEO_CODEC_AV1),
        ];

        for &(format, codec) in CODECS {
            let mut caps = CuvidDecodeCaps {
                e_codec_type: codec,
                e_chroma_format: CUDA_VIDEO_CHROMA_FORMAT_420,
                n_bit_depth_minus8: 0,
                ..CuvidDecodeCaps::default()
            };
            if (cuvid.cuvid_get_decoder_caps)(&mut caps) == CUDA_SUCCESS && caps.b_is_supported != 0
            {
                let mut caps10 = CuvidDecodeCaps {
                    e_codec_type: codec,
                    e_chroma_format: CUDA_VIDEO_CHROMA_FORMAT_420,
                    n_bit_depth_minus8: 2,
                    ..CuvidDecodeCaps::default()
                };
                if (cuvid.cuvid_get_decoder_caps)(&mut caps10) == CUDA_SUCCESS
                    && caps10.b_is_supported != 0
                {
                    supported.insert(format, vec![8, 10]);
                } else {
                    supported.insert(format, vec![8]);
                }
            }
        }
    }
}

unsafe fn probe_encode(
    nvenc: &NvencLib,
    ctx: *mut c_void,
    supported: &mut HashMap<VideoFormat, EncoderCapability>,
) {
    unsafe {
        let mut fn_list = NvEncApiFunctionList {
            version: NV_ENCODE_API_FUNCTION_LIST_VER,
            ..NvEncApiFunctionList::default()
        };
        if (nvenc.nv_encode_api_create_instance)(&mut fn_list) != NV_ENC_SUCCESS {
            return;
        }

        let Some(open_session_ex) = fn_list.nv_enc_open_encode_session_ex else {
            return;
        };
        let Some(get_guid_count) = fn_list.nv_enc_get_encode_guid_count else {
            return;
        };
        let Some(get_guids) = fn_list.nv_enc_get_encode_guids else {
            return;
        };
        // TODO: get encode profile list and store in capabilities
        let Some(_get_profile_count) = fn_list.nv_enc_get_encode_profile_guid_count else {
            return;
        };
        let Some(_get_profiles) = fn_list.nv_enc_get_encode_profile_guids else {
            return;
        };
        let Some(get_encode_caps) = fn_list.nv_enc_get_encode_caps else {
            return;
        };
        let Some(destroy) = fn_list.nv_enc_destroy_encoder else {
            return;
        };

        let mut params = NvEncOpenEncodeSessionExParams {
            version: NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
            device_type: NV_ENC_DEVICE_TYPE_CUDA,
            device: ctx,
            reserved: std::ptr::null_mut(),
            api_version: NVENCAPI_VERSION,
            _reserved1: [0u32; 253],
            _reserved2: [std::ptr::null_mut(); 64],
        };

        let mut encoder: *mut c_void = std::ptr::null_mut();
        let status = open_session_ex(&mut params, &mut encoder);
        if status != NV_ENC_SUCCESS {
            log::warn!("nvEncOpenEncodeSessionEx failed: {:#010x}", status);
            return;
        }

        let mut count = 0u32;
        let status = get_guid_count(encoder, &mut count);
        if status != NV_ENC_SUCCESS {
            log::warn!("nvEncGetEncodeGUIDCount failed: {:#010x}", status);
            destroy(encoder);
            return;
        }

        if count > 0 {
            let mut guids = vec![NvEncGuid::default(); count as usize];
            let mut actual = 0u32;
            if get_guids(encoder, guids.as_mut_ptr(), count, &mut actual) == NV_ENC_SUCCESS {
                for &guid in &guids[..actual.min(count) as usize] {
                    if let Some(format) = match guid {
                        NV_ENC_CODEC_H264_GUID => Some(VideoFormat::H264),
                        NV_ENC_CODEC_HEVC_GUID => Some(VideoFormat::Hevc),
                        NV_ENC_CODEC_AV1_GUID => Some(VideoFormat::Av1),
                        _ => None,
                    } {
                        let bit_depths = if query_cap(
                            encoder,
                            guid,
                            NV_ENC_CAPS_SUPPORT_10_BIT_ENCODE,
                            get_encode_caps,
                        ) {
                            vec![8, 10]
                        } else {
                            vec![8]
                        };

                        let b_frame_ref_mode = query_cap(
                            encoder,
                            guid,
                            NV_ENC_CAPS_SUPPORT_BFRAME_REF_MODE,
                            get_encode_caps,
                        );

                        supported.insert(
                            format,
                            EncoderCapability {
                                bit_depths,
                                b_frame_ref_mode,
                            },
                        );
                    };
                }
            }
        }

        destroy(encoder);
    }
}

unsafe fn query_cap(
    encoder: *mut c_void,
    guid: NvEncGuid,
    caps_to_query: u32,
    get_encode_caps: unsafe extern "C" fn(
        *mut c_void,
        NvEncGuid,
        *mut NvEncCapsParam,
        *mut i32,
    ) -> i32,
) -> bool {
    unsafe {
        let mut param = NvEncCapsParam {
            version: NV_ENC_CAPS_PARAM_VER,
            caps_to_query,
            reserved: [0u32; 62],
        };

        let mut caps_val: i32 = 0;
        get_encode_caps(encoder, guid, &mut param, &mut caps_val) == NV_ENC_SUCCESS && caps_val > 0
    }
}
