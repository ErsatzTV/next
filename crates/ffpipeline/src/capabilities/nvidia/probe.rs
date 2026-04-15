use std::collections::HashSet;
use std::ffi::c_void;

use libnvidia_sys::cuda::{CUDA_SUCCESS, CudaLib};
use libnvidia_sys::cuvid::{
    CUDA_VIDEO_CHROMA_FORMAT_420, CUDA_VIDEO_CODEC_AV1, CUDA_VIDEO_CODEC_H264,
    CUDA_VIDEO_CODEC_HEVC, CUDA_VIDEO_CODEC_MPEG2, CUDA_VIDEO_CODEC_VC1, CUDA_VIDEO_CODEC_VP8,
    CUDA_VIDEO_CODEC_VP9, CuvidDecodeCaps, CuvidLib,
};
use libnvidia_sys::nvenc::{
    NV_ENC_CODEC_H264_GUID, NV_ENC_CODEC_HEVC_GUID, NV_ENC_DEVICE_TYPE_CUDA,
    NV_ENC_HEVC_PROFILE_MAIN10_GUID, NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER, NV_ENC_SUCCESS,
    NV_ENCODE_API_FUNCTION_LIST_VER, NVENCAPI_VERSION, NvEncApiFunctionList, NvEncGuid,
    NvEncOpenEncodeSessionExParams, NvencLib,
};

use crate::capabilities::nvidia::NvidiaCapabilities;
use crate::error::FFPipelineError;
use crate::pipeline::VideoFormat;

impl NvidiaCapabilities {
    pub fn probe() -> Result<NvidiaCapabilities, FFPipelineError> {
        let mut supported_decoders = HashSet::new();
        let mut supported_encoders = HashSet::new();

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

unsafe fn probe_decode(cuvid: &CuvidLib, supported: &mut HashSet<(VideoFormat, u8)>) {
    unsafe {
        const CODECS: &[(VideoFormat, u32, bool)] = &[
            (VideoFormat::H264, CUDA_VIDEO_CODEC_H264, true),
            (VideoFormat::Hevc, CUDA_VIDEO_CODEC_HEVC, true),
            (VideoFormat::Mpeg2Video, CUDA_VIDEO_CODEC_MPEG2, false),
            (VideoFormat::Vc1, CUDA_VIDEO_CODEC_VC1, false),
            (VideoFormat::Vp8, CUDA_VIDEO_CODEC_VP8, false),
            (VideoFormat::Vp9, CUDA_VIDEO_CODEC_VP9, true),
            (VideoFormat::Av1, CUDA_VIDEO_CODEC_AV1, true),
        ];

        for &(format, codec, supports_10bit) in CODECS {
            let mut caps = CuvidDecodeCaps {
                e_codec_type: codec,
                e_chroma_format: CUDA_VIDEO_CHROMA_FORMAT_420,
                n_bit_depth_minus8: 0,
                ..CuvidDecodeCaps::default()
            };
            if (cuvid.cuvid_get_decoder_caps)(&mut caps) == CUDA_SUCCESS && caps.b_is_supported != 0
            {
                supported.insert((format, 8));

                if supports_10bit {
                    let mut caps10 = CuvidDecodeCaps {
                        e_codec_type: codec,
                        e_chroma_format: CUDA_VIDEO_CHROMA_FORMAT_420,
                        n_bit_depth_minus8: 2,
                        ..CuvidDecodeCaps::default()
                    };
                    if (cuvid.cuvid_get_decoder_caps)(&mut caps10) == CUDA_SUCCESS
                        && caps10.b_is_supported != 0
                    {
                        supported.insert((format, 10));
                    }
                }
            }
        }
    }
}

unsafe fn probe_encode(
    nvenc: &NvencLib,
    ctx: *mut c_void,
    supported: &mut HashSet<(VideoFormat, u8)>,
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
        let Some(get_profile_count) = fn_list.nv_enc_get_encode_profile_guid_count else {
            return;
        };
        let Some(get_profiles) = fn_list.nv_enc_get_encode_profile_guids else {
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
                    if guid == NV_ENC_CODEC_H264_GUID {
                        supported.insert((VideoFormat::H264, 8));
                    } else if guid == NV_ENC_CODEC_HEVC_GUID {
                        supported.insert((VideoFormat::Hevc, 8));
                        // Check whether the driver exposes the Main10 profile
                        if hevc_supports_main10(encoder, get_profile_count, get_profiles) {
                            supported.insert((VideoFormat::Hevc, 10));
                        }
                    }
                }
            }
        }

        destroy(encoder);
    }
}

unsafe fn hevc_supports_main10(
    encoder: *mut c_void,
    get_count: unsafe extern "C" fn(*mut c_void, NvEncGuid, *mut u32) -> i32,
    get_profiles: unsafe extern "C" fn(
        *mut c_void,
        NvEncGuid,
        *mut NvEncGuid,
        u32,
        *mut u32,
    ) -> i32,
) -> bool {
    unsafe {
        let mut count = 0u32;
        if get_count(encoder, NV_ENC_CODEC_HEVC_GUID, &mut count) != NV_ENC_SUCCESS || count == 0 {
            return false;
        }
        let mut profiles = vec![NvEncGuid::default(); count as usize];
        let mut actual = 0u32;
        if get_profiles(
            encoder,
            NV_ENC_CODEC_HEVC_GUID,
            profiles.as_mut_ptr(),
            count,
            &mut actual,
        ) != NV_ENC_SUCCESS
        {
            return false;
        }
        profiles[..actual.min(count) as usize].contains(&NV_ENC_HEVC_PROFILE_MAIN10_GUID)
    }
}
