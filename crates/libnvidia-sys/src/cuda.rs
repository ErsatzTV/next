use std::ffi::c_void;

use libloading::Library;

pub const CUDA_SUCCESS: i32 = 0;

pub struct CudaLib {
    _lib: Library,
    pub cu_init: unsafe extern "C" fn(flags: u32) -> i32,
    pub cu_device_get: unsafe extern "C" fn(device: *mut i32, ordinal: i32) -> i32,
    pub cu_ctx_create: unsafe extern "C" fn(pctx: *mut *mut c_void, flags: u32, dev: i32) -> i32,
    pub cu_ctx_destroy: unsafe extern "C" fn(ctx: *mut c_void) -> i32,
}

impl CudaLib {
    pub fn load() -> Result<Self, libloading::Error> {
        #[cfg(target_os = "linux")]
        let name = "libcuda.so.1";
        #[cfg(target_os = "windows")]
        let name = "nvcuda.dll";
        unsafe {
            let lib = Library::new(name)?;
            let cu_init = *lib.get(b"cuInit\0")?;
            let cu_device_get = *lib.get(b"cuDeviceGet\0")?;
            let cu_ctx_create = *lib.get(b"cuCtxCreate_v2\0")?;
            let cu_ctx_destroy = *lib.get(b"cuCtxDestroy_v2\0")?;
            Ok(Self {
                _lib: lib,
                cu_init,
                cu_device_get,
                cu_ctx_create,
                cu_ctx_destroy,
            })
        }
    }
}
