use std::ffi::c_void;

use libloading::Library;

const NVENCAPI_MAJOR_VERSION: u32 = 12;
const NVENCAPI_MINOR_VERSION: u32 = 2;
pub const NVENCAPI_VERSION: u32 = NVENCAPI_MAJOR_VERSION | (NVENCAPI_MINOR_VERSION << 24);

const fn nvencapi_struct_version(ver: u32) -> u32 {
    NVENCAPI_VERSION | (ver << 16) | (0x7 << 28)
}

pub const NV_ENCODE_API_FUNCTION_LIST_VER: u32 = nvencapi_struct_version(2);
pub const NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER: u32 = nvencapi_struct_version(1);

pub const NV_ENC_DEVICE_TYPE_CUDA: u32 = 1;
pub const NV_ENC_SUCCESS: i32 = 0;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Default)]
pub struct NvEncGuid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

pub const NV_ENC_CODEC_H264_GUID: NvEncGuid = NvEncGuid {
    data1: 0x6bc82762,
    data2: 0x4e63,
    data3: 0x4ca4,
    data4: [0xaa, 0x85, 0x1e, 0x50, 0xf3, 0x21, 0xf6, 0xbf],
};

pub const NV_ENC_CODEC_HEVC_GUID: NvEncGuid = NvEncGuid {
    data1: 0x790cdc88,
    data2: 0x4522,
    data3: 0x4d7b,
    data4: [0x94, 0x25, 0xbd, 0xa9, 0x97, 0x5f, 0x76, 0x03],
};

pub const NV_ENC_HEVC_PROFILE_MAIN10_GUID: NvEncGuid = NvEncGuid {
    data1: 0xfa4d2b6c,
    data2: 0x3a5b,
    data3: 0x411a,
    data4: [0x80, 0x18, 0x0a, 0x3f, 0x5e, 0x3c, 0x9b, 0xe5],
};

#[repr(C)]
pub struct NvEncApiFunctionList {
    pub version: u32,
    pub reserved: u32,
    pub _slot0: usize, // nvEncOpenEncodeSession (deprecated, unused)
    pub nv_enc_get_encode_guid_count: Option<unsafe extern "C" fn(*mut c_void, *mut u32) -> i32>,
    pub nv_enc_get_encode_profile_guid_count:
        Option<unsafe extern "C" fn(*mut c_void, NvEncGuid, *mut u32) -> i32>,
    pub nv_enc_get_encode_profile_guids:
        Option<unsafe extern "C" fn(*mut c_void, NvEncGuid, *mut NvEncGuid, u32, *mut u32) -> i32>,
    pub nv_enc_get_encode_guids:
        Option<unsafe extern "C" fn(*mut c_void, *mut NvEncGuid, u32, *mut u32) -> i32>,
    pub _slots5_26: [usize; 22],
    pub nv_enc_destroy_encoder: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub _slot28: usize,
    pub nv_enc_open_encode_session_ex:
        Option<unsafe extern "C" fn(*mut NvEncOpenEncodeSessionExParams, *mut *mut c_void) -> i32>,
    pub _tail: [usize; 289], // pads to 2560 bytes
}

impl Default for NvEncApiFunctionList {
    fn default() -> Self {
        // Safety: all fields are either numeric (0), Option<fn> (null = None),
        // or usize padding (0) — zeroed bytes are valid for all of them.
        unsafe { std::mem::zeroed() }
    }
}

const _: () = assert!(size_of::<NvEncApiFunctionList>() == 2560);

#[repr(C)]
pub struct NvEncOpenEncodeSessionExParams {
    pub version: u32,          // NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER
    pub device_type: u32,      // NV_ENC_DEVICE_TYPE_CUDA
    pub device: *mut c_void,   // CUcontext
    pub reserved: *mut c_void, // NULL
    pub api_version: u32,      // NVENCAPI_VERSION
    pub _reserved1: [u32; 253],
    pub _reserved2: [*mut c_void; 64],
}

pub struct NvencLib {
    _lib: Library,
    pub nv_encode_api_create_instance: unsafe extern "C" fn(*mut NvEncApiFunctionList) -> i32,
}

impl NvencLib {
    pub fn load() -> Result<Self, libloading::Error> {
        #[cfg(target_os = "linux")]
        let name = "libnvidia-encode.so.1";
        #[cfg(target_os = "windows")]
        let name = "nvEncodeAPI64.dll";
        unsafe {
            let lib = Library::new(name)?;
            let nv_encode_api_create_instance = *lib.get(b"NvEncodeAPICreateInstance\0")?;
            Ok(Self {
                _lib: lib,
                nv_encode_api_create_instance,
            })
        }
    }
}
