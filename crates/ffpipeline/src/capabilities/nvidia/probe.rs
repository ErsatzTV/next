use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, Once};
use std::time::Duration;

use libnvidia_sys::cuda::{CUDA_SUCCESS, CudaLib};
use libnvidia_sys::cuvid::{
    CUDA_VIDEO_CHROMA_FORMAT_420, CUDA_VIDEO_CODEC_AV1, CUDA_VIDEO_CODEC_H264,
    CUDA_VIDEO_CODEC_HEVC, CUDA_VIDEO_CODEC_MPEG2, CUDA_VIDEO_CODEC_VC1, CUDA_VIDEO_CODEC_VP8,
    CUDA_VIDEO_CODEC_VP9, CuvidDecodeCaps, CuvidLib,
};
use libnvidia_sys::nvenc::{
    NV_ENC_CAPS_NUM_MAX_BFRAMES, NV_ENC_CAPS_PARAM_VER, NV_ENC_CAPS_SUPPORT_10_BIT_ENCODE,
    NV_ENC_CAPS_SUPPORT_BFRAME_REF_MODE, NV_ENC_CODEC_AV1_GUID, NV_ENC_CODEC_H264_GUID,
    NV_ENC_CODEC_HEVC_GUID, NV_ENC_DEVICE_TYPE_CUDA, NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
    NV_ENC_SUCCESS, NV_ENCODE_API_FUNCTION_LIST_VER, NVENCAPI_VERSION, NvEncApiFunctionList,
    NvEncCapsParam, NvEncGuid, NvEncOpenEncodeSessionExParams, NvencLib,
};

use crate::capabilities::nvidia::{EncoderCapability, NvidiaCapabilities};
use crate::error::FFPipelineError;
use crate::pipeline::VideoFormat;

/// Pointers are stored as `usize` to make this `Send`; only used after the probe returns.
struct Teardown {
    cuda: CudaLib,
    ctx: usize,
    encoder: Option<(NvencLib, NvEncDestroyFn, usize)>,
    _active: ActiveProbe,
}

type NvEncDestroyFn = unsafe extern "C" fn(*mut c_void) -> i32;

impl Teardown {
    fn run(self) {
        unsafe {
            if let Some((_nvenc, destroy, encoder)) = &self.encoder {
                destroy(*encoder as *mut c_void);
            }
            (self.cuda.cu_ctx_destroy)(self.ctx as *mut c_void);
        }
    }
}

// nvEncDestroyEncoder can block ~250 ms (Linux driver 580), longer than the whole probe
fn finish_probe(teardown: Teardown) {
    if BACKGROUND_TEARDOWN.load(Ordering::Relaxed) {
        std::thread::spawn(move || teardown.run());
    } else {
        teardown.run();
    }
}

/// Counts probes from entry until teardown ends. Exiting while one is in the driver can
/// crash in the driver's exit handlers, so shutdown waits for them.
struct ProbeGate {
    state: Mutex<GateState>,
    idle: Condvar,
}

struct GateState {
    closed: bool,
    active: usize,
}

impl ProbeGate {
    const fn new() -> Self {
        Self {
            state: Mutex::new(GateState {
                closed: false,
                active: 0,
            }),
            idle: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn enter(&'static self) -> Option<ActiveProbe> {
        let mut state = self.lock();
        if state.closed {
            return None;
        }
        state.active += 1;
        Some(ActiveProbe(self))
    }

    /// False if probes are still running at `limit`.
    fn close_and_wait(&self, limit: Duration) -> bool {
        let mut state = self.lock();
        state.closed = true;
        let (state, _) = self
            .idle
            .wait_timeout_while(state, limit, |state| state.active > 0)
            .unwrap_or_else(|e| e.into_inner());
        state.active == 0
    }
}

struct ActiveProbe(&'static ProbeGate);

impl Drop for ActiveProbe {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.active -= 1;
        if state.active == 0 {
            self.0.idle.notify_all();
        }
    }
}

// off unless the process holds `NvidiaProbeShutdown`, which waits for it at exit
static BACKGROUND_TEARDOWN: AtomicBool = AtomicBool::new(false);
static GATE: ProbeGate = ProbeGate::new();

fn enter_driver() -> Result<ActiveProbe, FFPipelineError> {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| unsafe {
        libc::atexit(wait_for_probes);
    });

    GATE.enter()
        .ok_or_else(|| FFPipelineError::NvidiaCapabilitiesError("process is exiting".into()))
}

extern "C" fn wait_for_probes() {
    if !GATE.close_and_wait(Duration::from_secs(5)) {
        eprintln!("NVIDIA capability probe is stuck in the driver; exiting");
        // skip library destructors (unix) and DLL detach (Windows); both can crash or
        // deadlock under a thread stuck in the driver
        #[cfg(unix)]
        unsafe {
            libc::_exit(1)
        };
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
            TerminateProcess(GetCurrentProcess(), 1);
        }
    }
}

unsafe fn init_device(cuda: &CudaLib) -> Result<(i32, Option<[u8; 16]>), FFPipelineError> {
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

        let mut uuid = [0u8; 16];
        let device_uuid =
            ((cuda.cu_device_get_uuid)(&mut uuid, device) == CUDA_SUCCESS).then_some(uuid);

        Ok((device, device_uuid))
    }
}

fn load_cuda() -> Result<CudaLib, FFPipelineError> {
    CudaLib::load().map_err(|e| {
        FFPipelineError::NvidiaCapabilitiesError(format!("failed to load libcuda: {e}"))
    })
}

impl NvidiaCapabilities {
    pub(super) fn enable_background_teardown() {
        BACKGROUND_TEARDOWN.store(true, Ordering::Relaxed);
    }

    pub(super) fn shutdown_probes() {
        wait_for_probes();
    }

    pub fn probe() -> Result<NvidiaCapabilities, FFPipelineError> {
        let active = enter_driver()?;
        let mut supported_decoders = HashMap::new();
        let mut supported_encoders = HashMap::new();

        let cuda = load_cuda()?;

        let device_uuid = unsafe {
            let (device, device_uuid) = init_device(&cuda)?;

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

            let encoder = match NvencLib::load() {
                Ok(nvenc) => probe_encode(&nvenc, ctx, &mut supported_encoders)
                    .map(|(destroy, encoder)| (nvenc, destroy, encoder as usize)),
                Err(_) => {
                    log::warn!("libnvidia-encode not available; skipping encode capability probe");
                    None
                }
            };

            // cuCtxCreate made it current on this (often pooled) thread; it's destroyed on another
            (cuda.cu_ctx_pop_current)(&mut std::ptr::null_mut());
            finish_probe(Teardown {
                cuda,
                ctx: ctx as usize,
                encoder,
                _active: active,
            });

            device_uuid
        };

        Ok(NvidiaCapabilities {
            supported_decoders,
            supported_encoders,
            device_uuid,
        })
    }

    /// Holds a gate slot through Vulkan cleanup, which also runs in the driver.
    pub(crate) fn probe_vulkan()
    -> Result<crate::capabilities::vulkan::VulkanCapabilities, FFPipelineError> {
        let _active = enter_driver()?;
        let device_uuid = {
            let cuda = load_cuda()?;
            unsafe { init_device(&cuda)?.1 }
        };
        crate::capabilities::vulkan::VulkanCapabilities::probe_for_nvidia(device_uuid)
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
) -> Option<(NvEncDestroyFn, *mut c_void)> {
    unsafe {
        let mut fn_list = NvEncApiFunctionList {
            version: NV_ENCODE_API_FUNCTION_LIST_VER,
            ..NvEncApiFunctionList::default()
        };
        if (nvenc.nv_encode_api_create_instance)(&mut fn_list) != NV_ENC_SUCCESS {
            return None;
        }

        let open_session_ex = fn_list.nv_enc_open_encode_session_ex?;
        let get_guid_count = fn_list.nv_enc_get_encode_guid_count?;
        let get_guids = fn_list.nv_enc_get_encode_guids?;
        // TODO: get encode profile list and store in capabilities
        let _get_profile_count = fn_list.nv_enc_get_encode_profile_guid_count?;
        let _get_profiles = fn_list.nv_enc_get_encode_profile_guids?;
        let get_encode_caps = fn_list.nv_enc_get_encode_caps?;
        let destroy = fn_list.nv_enc_destroy_encoder?;

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
            return None;
        }

        let mut count = 0u32;
        let status = get_guid_count(encoder, &mut count);
        if status != NV_ENC_SUCCESS {
            log::warn!("nvEncGetEncodeGUIDCount failed: {:#010x}", status);
            return Some((destroy, encoder));
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

                        let max_b_frames = query_cap_value(
                            encoder,
                            guid,
                            NV_ENC_CAPS_NUM_MAX_BFRAMES,
                            get_encode_caps,
                        )
                        .max(0) as u32;

                        supported.insert(
                            format,
                            EncoderCapability {
                                bit_depths,
                                b_frame_ref_mode,
                                max_b_frames,
                            },
                        );
                    };
                }
            }
        }

        Some((destroy, encoder))
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
    unsafe { query_cap_value(encoder, guid, caps_to_query, get_encode_caps) > 0 }
}

unsafe fn query_cap_value(
    encoder: *mut c_void,
    guid: NvEncGuid,
    caps_to_query: u32,
    get_encode_caps: unsafe extern "C" fn(
        *mut c_void,
        NvEncGuid,
        *mut NvEncCapsParam,
        *mut i32,
    ) -> i32,
) -> i32 {
    unsafe {
        let mut param = NvEncCapsParam {
            version: NV_ENC_CAPS_PARAM_VER,
            caps_to_query,
            reserved: [0u32; 62],
        };

        let mut caps_val: i32 = 0;
        if get_encode_caps(encoder, guid, &mut param, &mut caps_val) == NV_ENC_SUCCESS {
            caps_val
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    fn gate() -> &'static ProbeGate {
        Box::leak(Box::new(ProbeGate::new()))
    }

    #[test]
    fn close_waits_for_a_probe_that_hands_off_its_teardown_after_shutdown_starts() {
        let gate = gate();
        let active = gate.enter().unwrap();
        let (release, paused) = mpsc::channel::<()>();
        let probe = std::thread::spawn(move || {
            paused.recv().unwrap();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                drop(active);
            })
        });

        let closer = std::thread::spawn(|| {
            let start = Instant::now();
            (gate.close_and_wait(Duration::from_secs(5)), start.elapsed())
        });
        while !gate.lock().closed {
            std::thread::yield_now();
        }
        assert!(gate.enter().is_none());
        release.send(()).unwrap();

        let (idle, waited) = closer.join().unwrap();
        probe.join().unwrap().join().unwrap();
        assert!(idle);
        assert!(waited >= Duration::from_millis(50));
    }

    #[test]
    fn close_gives_up_at_the_limit() {
        let gate = gate();
        let _active = gate.enter().unwrap();
        assert!(!gate.close_and_wait(Duration::from_millis(20)));
    }

    #[test]
    fn close_returns_immediately_when_idle() {
        let gate = gate();
        drop(gate.enter().unwrap());
        assert!(gate.close_and_wait(Duration::ZERO));
        assert!(gate.enter().is_none());
    }
}
