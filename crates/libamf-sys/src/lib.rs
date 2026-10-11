// AMF is a C++ API, but its headers also define each interface as a C struct with a
// vtable pointer. Mirroring those lets the runtime load through libloading without a
// C++ toolchain. Only slots the probe calls have real signatures; the rest are opaque
// function pointers so the offsets still match.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::{c_char, c_long, c_void};

pub type amf_bool = u8;
pub type amf_int32 = i32;
pub type amf_int64 = i64;
pub type amf_uint64 = u64;
pub type amf_long = c_long;
pub type amf_size = usize;

/// `wchar_t` is 16-bit on Windows and 32-bit everywhere else.
#[cfg(target_os = "windows")]
pub type amf_wchar = u16;
#[cfg(not(target_os = "windows"))]
pub type amf_wchar = u32;

pub fn wide(s: &str) -> Vec<amf_wchar> {
    #[cfg(target_os = "windows")]
    let chars = s.encode_utf16().map(|c| c as amf_wchar);
    #[cfg(not(target_os = "windows"))]
    let chars = s.chars().map(|c| c as amf_wchar);
    chars.chain(std::iter::once(0)).collect()
}

/// # Safety
/// `ptr` must be null or point to a NUL-terminated string.
pub unsafe fn from_wide(ptr: *const amf_wchar) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0;
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    #[cfg(target_os = "windows")]
    {
        String::from_utf16_lossy(slice)
    }
    #[cfg(not(target_os = "windows"))]
    {
        slice
            .iter()
            .map(|&c| char::from_u32(c).unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    }
}

pub const fn amf_make_full_version(major: u64, minor: u64, release: u64, build: u64) -> u64 {
    (major << 48) | (minor << 32) | (release << 16) | build
}

/// Version of the SDK headers these bindings follow
pub const AMF_FULL_VERSION: u64 = amf_make_full_version(1, 5, 2, 0);

pub const fn amf_version_parts(version: u64) -> (u16, u16, u16, u16) {
    (
        (version >> 48) as u16,
        (version >> 32) as u16,
        (version >> 16) as u16,
        version as u16,
    )
}

pub type AMF_RESULT = i32;
pub const AMF_OK: AMF_RESULT = 0;
pub const AMF_FAIL: AMF_RESULT = 1;
pub const AMF_UNEXPECTED: AMF_RESULT = 2;
pub const AMF_ACCESS_DENIED: AMF_RESULT = 3;
pub const AMF_INVALID_ARG: AMF_RESULT = 4;
pub const AMF_OUT_OF_RANGE: AMF_RESULT = 5;
pub const AMF_OUT_OF_MEMORY: AMF_RESULT = 6;
pub const AMF_INVALID_POINTER: AMF_RESULT = 7;
pub const AMF_NO_INTERFACE: AMF_RESULT = 8;
pub const AMF_NOT_IMPLEMENTED: AMF_RESULT = 9;
pub const AMF_NOT_SUPPORTED: AMF_RESULT = 10;
pub const AMF_NOT_FOUND: AMF_RESULT = 11;
pub const AMF_ALREADY_INITIALIZED: AMF_RESULT = 12;
pub const AMF_NOT_INITIALIZED: AMF_RESULT = 13;
pub const AMF_INVALID_FORMAT: AMF_RESULT = 14;
pub const AMF_WRONG_STATE: AMF_RESULT = 15;
pub const AMF_FILE_NOT_OPEN: AMF_RESULT = 16;
pub const AMF_NO_DEVICE: AMF_RESULT = 17;
pub const AMF_DIRECTX_FAILED: AMF_RESULT = 18;
pub const AMF_OPENCL_FAILED: AMF_RESULT = 19;
pub const AMF_GLX_FAILED: AMF_RESULT = 20;
pub const AMF_XV_FAILED: AMF_RESULT = 21;
pub const AMF_ALSA_FAILED: AMF_RESULT = 22;
pub const AMF_EOF: AMF_RESULT = 23;
pub const AMF_REPEAT: AMF_RESULT = 24;
pub const AMF_INPUT_FULL: AMF_RESULT = 25;
pub const AMF_RESOLUTION_CHANGED: AMF_RESULT = 26;
pub const AMF_RESOLUTION_UPDATED: AMF_RESULT = 27;
pub const AMF_INVALID_DATA_TYPE: AMF_RESULT = 28;
pub const AMF_INVALID_RESOLUTION: AMF_RESULT = 29;
pub const AMF_CODEC_NOT_SUPPORTED: AMF_RESULT = 30;
pub const AMF_SURFACE_FORMAT_NOT_SUPPORTED: AMF_RESULT = 31;
pub const AMF_SURFACE_MUST_BE_SHARED: AMF_RESULT = 32;
pub const AMF_DECODER_NOT_PRESENT: AMF_RESULT = 33;
pub const AMF_DECODER_SURFACE_ALLOCATION_FAILED: AMF_RESULT = 34;
pub const AMF_DECODER_NO_FREE_SURFACES: AMF_RESULT = 35;
pub const AMF_ENCODER_NOT_PRESENT: AMF_RESULT = 36;
pub const AMF_DEM_ERROR: AMF_RESULT = 37;
pub const AMF_DEM_PROPERTY_READONLY: AMF_RESULT = 38;
pub const AMF_DEM_REMOTE_DISPLAY_CREATE_FAILED: AMF_RESULT = 39;
pub const AMF_DEM_START_ENCODING_FAILED: AMF_RESULT = 40;
pub const AMF_DEM_QUERY_OUTPUT_FAILED: AMF_RESULT = 41;
pub const AMF_TAN_CLIPPING_WAS_REQUIRED: AMF_RESULT = 42;
pub const AMF_TAN_UNSUPPORTED_VERSION: AMF_RESULT = 43;
pub const AMF_NEED_MORE_INPUT: AMF_RESULT = 44;
pub const AMF_VULKAN_FAILED: AMF_RESULT = 45;

pub fn amf_result_name(result: AMF_RESULT) -> String {
    let name = match result {
        AMF_OK => "AMF_OK",
        AMF_FAIL => "AMF_FAIL",
        AMF_UNEXPECTED => "AMF_UNEXPECTED",
        AMF_ACCESS_DENIED => "AMF_ACCESS_DENIED",
        AMF_INVALID_ARG => "AMF_INVALID_ARG",
        AMF_OUT_OF_RANGE => "AMF_OUT_OF_RANGE",
        AMF_OUT_OF_MEMORY => "AMF_OUT_OF_MEMORY",
        AMF_INVALID_POINTER => "AMF_INVALID_POINTER",
        AMF_NO_INTERFACE => "AMF_NO_INTERFACE",
        AMF_NOT_IMPLEMENTED => "AMF_NOT_IMPLEMENTED",
        AMF_NOT_SUPPORTED => "AMF_NOT_SUPPORTED",
        AMF_NOT_FOUND => "AMF_NOT_FOUND",
        AMF_ALREADY_INITIALIZED => "AMF_ALREADY_INITIALIZED",
        AMF_NOT_INITIALIZED => "AMF_NOT_INITIALIZED",
        AMF_INVALID_FORMAT => "AMF_INVALID_FORMAT",
        AMF_WRONG_STATE => "AMF_WRONG_STATE",
        AMF_NO_DEVICE => "AMF_NO_DEVICE",
        AMF_DIRECTX_FAILED => "AMF_DIRECTX_FAILED",
        AMF_OPENCL_FAILED => "AMF_OPENCL_FAILED",
        AMF_CODEC_NOT_SUPPORTED => "AMF_CODEC_NOT_SUPPORTED",
        AMF_SURFACE_FORMAT_NOT_SUPPORTED => "AMF_SURFACE_FORMAT_NOT_SUPPORTED",
        AMF_DECODER_NOT_PRESENT => "AMF_DECODER_NOT_PRESENT",
        AMF_ENCODER_NOT_PRESENT => "AMF_ENCODER_NOT_PRESENT",
        AMF_VULKAN_FAILED => "AMF_VULKAN_FAILED",
        _ => return format!("AMF_RESULT({result})"),
    };
    name.to_string()
}

pub type AMF_SURFACE_FORMAT = i32;
pub const AMF_SURFACE_UNKNOWN: AMF_SURFACE_FORMAT = 0;
pub const AMF_SURFACE_NV12: AMF_SURFACE_FORMAT = 1;
pub const AMF_SURFACE_YV12: AMF_SURFACE_FORMAT = 2;
pub const AMF_SURFACE_BGRA: AMF_SURFACE_FORMAT = 3;
pub const AMF_SURFACE_ARGB: AMF_SURFACE_FORMAT = 4;
pub const AMF_SURFACE_RGBA: AMF_SURFACE_FORMAT = 5;
pub const AMF_SURFACE_GRAY8: AMF_SURFACE_FORMAT = 6;
pub const AMF_SURFACE_YUV420P: AMF_SURFACE_FORMAT = 7;
pub const AMF_SURFACE_U8V8: AMF_SURFACE_FORMAT = 8;
pub const AMF_SURFACE_YUY2: AMF_SURFACE_FORMAT = 9;
pub const AMF_SURFACE_P010: AMF_SURFACE_FORMAT = 10;
pub const AMF_SURFACE_RGBA_F16: AMF_SURFACE_FORMAT = 11;
pub const AMF_SURFACE_UYVY: AMF_SURFACE_FORMAT = 12;
pub const AMF_SURFACE_R10G10B10A2: AMF_SURFACE_FORMAT = 13;
pub const AMF_SURFACE_Y210: AMF_SURFACE_FORMAT = 14;
pub const AMF_SURFACE_AYUV: AMF_SURFACE_FORMAT = 15;
pub const AMF_SURFACE_Y410: AMF_SURFACE_FORMAT = 16;
pub const AMF_SURFACE_Y416: AMF_SURFACE_FORMAT = 17;
pub const AMF_SURFACE_GRAY32: AMF_SURFACE_FORMAT = 18;
pub const AMF_SURFACE_P012: AMF_SURFACE_FORMAT = 19;
pub const AMF_SURFACE_P016: AMF_SURFACE_FORMAT = 20;
pub const AMF_SURFACE_Y216: AMF_SURFACE_FORMAT = 21;
pub const AMF_SURFACE_R16G16: AMF_SURFACE_FORMAT = 22;
pub const AMF_SURFACE_R24G8: AMF_SURFACE_FORMAT = 23;
pub const AMF_SURFACE_R32: AMF_SURFACE_FORMAT = 24;
pub const AMF_SURFACE_R16: AMF_SURFACE_FORMAT = 25;

pub fn amf_surface_format_name(format: AMF_SURFACE_FORMAT) -> String {
    let name = match format {
        AMF_SURFACE_UNKNOWN => "UNKNOWN",
        AMF_SURFACE_NV12 => "NV12",
        AMF_SURFACE_YV12 => "YV12",
        AMF_SURFACE_BGRA => "BGRA",
        AMF_SURFACE_ARGB => "ARGB",
        AMF_SURFACE_RGBA => "RGBA",
        AMF_SURFACE_GRAY8 => "GRAY8",
        AMF_SURFACE_YUV420P => "YUV420P",
        AMF_SURFACE_U8V8 => "U8V8",
        AMF_SURFACE_YUY2 => "YUY2",
        AMF_SURFACE_P010 => "P010",
        AMF_SURFACE_RGBA_F16 => "RGBA_F16",
        AMF_SURFACE_UYVY => "UYVY",
        AMF_SURFACE_R10G10B10A2 => "R10G10B10A2",
        AMF_SURFACE_Y210 => "Y210",
        AMF_SURFACE_AYUV => "AYUV",
        AMF_SURFACE_Y410 => "Y410",
        AMF_SURFACE_Y416 => "Y416",
        AMF_SURFACE_GRAY32 => "GRAY32",
        AMF_SURFACE_P012 => "P012",
        AMF_SURFACE_P016 => "P016",
        AMF_SURFACE_Y216 => "Y216",
        AMF_SURFACE_R16G16 => "R16G16",
        AMF_SURFACE_R24G8 => "R24G8",
        AMF_SURFACE_R32 => "R32",
        AMF_SURFACE_R16 => "R16",
        _ => return format!("AMF_SURFACE_FORMAT({format})"),
    };
    name.to_string()
}

pub type AMF_MEMORY_TYPE = i32;
pub const AMF_MEMORY_UNKNOWN: AMF_MEMORY_TYPE = 0;
pub const AMF_MEMORY_HOST: AMF_MEMORY_TYPE = 1;
pub const AMF_MEMORY_DX9: AMF_MEMORY_TYPE = 2;
pub const AMF_MEMORY_DX11: AMF_MEMORY_TYPE = 3;
pub const AMF_MEMORY_OPENCL: AMF_MEMORY_TYPE = 4;
pub const AMF_MEMORY_OPENGL: AMF_MEMORY_TYPE = 5;
pub const AMF_MEMORY_XV: AMF_MEMORY_TYPE = 6;
pub const AMF_MEMORY_GRALLOC: AMF_MEMORY_TYPE = 7;
pub const AMF_MEMORY_COMPUTE_FOR_DX9: AMF_MEMORY_TYPE = 8;
pub const AMF_MEMORY_COMPUTE_FOR_DX11: AMF_MEMORY_TYPE = 9;
pub const AMF_MEMORY_VULKAN: AMF_MEMORY_TYPE = 10;
pub const AMF_MEMORY_DX12: AMF_MEMORY_TYPE = 11;

pub fn amf_memory_type_name(memory_type: AMF_MEMORY_TYPE) -> String {
    let name = match memory_type {
        AMF_MEMORY_UNKNOWN => "UNKNOWN",
        AMF_MEMORY_HOST => "HOST",
        AMF_MEMORY_DX9 => "DX9",
        AMF_MEMORY_DX11 => "DX11",
        AMF_MEMORY_OPENCL => "OPENCL",
        AMF_MEMORY_OPENGL => "OPENGL",
        AMF_MEMORY_XV => "XV",
        AMF_MEMORY_GRALLOC => "GRALLOC",
        AMF_MEMORY_COMPUTE_FOR_DX9 => "COMPUTE_FOR_DX9",
        AMF_MEMORY_COMPUTE_FOR_DX11 => "COMPUTE_FOR_DX11",
        AMF_MEMORY_VULKAN => "VULKAN",
        AMF_MEMORY_DX12 => "DX12",
        _ => return format!("AMF_MEMORY_TYPE({memory_type})"),
    };
    name.to_string()
}

pub type AMF_DX_VERSION = i32;
pub const AMF_DX9: AMF_DX_VERSION = 90;
pub const AMF_DX9_EX: AMF_DX_VERSION = 91;
pub const AMF_DX11_0: AMF_DX_VERSION = 110;
pub const AMF_DX11_1: AMF_DX_VERSION = 111;
pub const AMF_DX12: AMF_DX_VERSION = 120;

pub type AMF_ACCELERATION_TYPE = i32;
pub const AMF_ACCEL_NOT_SUPPORTED: AMF_ACCELERATION_TYPE = -1;
pub const AMF_ACCEL_HARDWARE: AMF_ACCELERATION_TYPE = 0;
pub const AMF_ACCEL_GPU: AMF_ACCELERATION_TYPE = 1;
pub const AMF_ACCEL_SOFTWARE: AMF_ACCELERATION_TYPE = 2;

pub type AMF_VARIANT_TYPE = i32;
pub const AMF_VARIANT_EMPTY: AMF_VARIANT_TYPE = 0;
pub const AMF_VARIANT_BOOL: AMF_VARIANT_TYPE = 1;
pub const AMF_VARIANT_INT64: AMF_VARIANT_TYPE = 2;
pub const AMF_VARIANT_DOUBLE: AMF_VARIANT_TYPE = 3;
pub const AMF_VARIANT_RECT: AMF_VARIANT_TYPE = 4;
pub const AMF_VARIANT_SIZE: AMF_VARIANT_TYPE = 5;
pub const AMF_VARIANT_POINT: AMF_VARIANT_TYPE = 6;
pub const AMF_VARIANT_RATE: AMF_VARIANT_TYPE = 7;
pub const AMF_VARIANT_RATIO: AMF_VARIANT_TYPE = 8;
pub const AMF_VARIANT_COLOR: AMF_VARIANT_TYPE = 9;
pub const AMF_VARIANT_STRING: AMF_VARIANT_TYPE = 10;
pub const AMF_VARIANT_WSTRING: AMF_VARIANT_TYPE = 11;
pub const AMF_VARIANT_INTERFACE: AMF_VARIANT_TYPE = 12;
pub const AMF_VARIANT_FLOAT: AMF_VARIANT_TYPE = 13;
pub const AMF_VARIANT_FLOAT_SIZE: AMF_VARIANT_TYPE = 14;
pub const AMF_VARIANT_FLOAT_POINT2D: AMF_VARIANT_TYPE = 15;
pub const AMF_VARIANT_FLOAT_POINT3D: AMF_VARIANT_TYPE = 16;
pub const AMF_VARIANT_FLOAT_VECTOR4D: AMF_VARIANT_TYPE = 17;

// Only the components ffmpeg creates are listed. Stock ffmpeg uses H.264, HEVC, VP9
// and AV1; MPEG-2 and VC-1 need the etv mpeg2_amf/vc1_amf patch. The
// SDK also has MPEG-4, WMV3 and MJPEG components, but nothing uses them.
pub const AMFVideoDecoderUVD_MPEG2: &str = "AMFVideoDecoderUVD_MPEG2";
pub const AMFVideoDecoderUVD_VC1: &str = "AMFVideoDecoderUVD_VC1";
pub const AMFVideoDecoderUVD_H264_AVC: &str = "AMFVideoDecoderUVD_H264_AVC";
pub const AMFVideoDecoderHW_H265_HEVC: &str = "AMFVideoDecoderHW_H265_HEVC";
/// Deprecated in the SDK, but ffmpeg still creates this for 10-bit HEVC
pub const AMFVideoDecoderHW_H265_MAIN10: &str = "AMFVideoDecoderHW_H265_MAIN10";
pub const AMFVideoDecoderHW_VP9: &str = "AMFVideoDecoderHW_VP9";
/// Deprecated in the SDK, but ffmpeg still creates this for 10-bit VP9
pub const AMFVideoDecoderHW_VP9_10BIT: &str = "AMFVideoDecoderHW_VP9_10BIT";
pub const AMFVideoDecoderHW_AV1: &str = "AMFVideoDecoderHW_AV1";
pub const AMFVideoEncoderVCE_AVC: &str = "AMFVideoEncoderVCE_AVC";
pub const AMFVideoEncoder_HEVC: &str = "AMFVideoEncoderHW_HEVC";
pub const AMFVideoEncoder_AV1: &str = "AMFVideoEncoderHW_AV1";
pub const AMFVideoConverter: &str = "AMFVideoConverter";

pub const AMF_VIDEO_ENCODER_PROFILE: &str = "Profile";
pub const AMF_VIDEO_ENCODER_COLOR_BIT_DEPTH: &str = "ColorBitDepth";
pub const AMF_VIDEO_ENCODER_HEVC_PROFILE: &str = "HevcProfile";
pub const AMF_VIDEO_ENCODER_HEVC_COLOR_BIT_DEPTH: &str = "HevcColorBitDepth";
pub const AMF_VIDEO_ENCODER_AV1_PROFILE: &str = "Av1Profile";
pub const AMF_VIDEO_ENCODER_AV1_COLOR_BIT_DEPTH: &str = "Av1ColorBitDepth";

pub const AMF_VIDEO_ENCODER_CAP_MAX_PROFILE: &str = "MaxProfile";
pub const AMF_VIDEO_ENCODER_CAP_MAX_LEVEL: &str = "MaxLevel";
pub const AMF_VIDEO_ENCODER_CAP_BFRAMES: &str = "BFrames";
pub const AMF_VIDEO_ENCODER_CAP_NUM_OF_HW_INSTANCES: &str = "NumOfHwInstances";
pub const AMF_VIDEO_ENCODER_HEVC_CAP_MAX_PROFILE: &str = "HevcMaxProfile";
pub const AMF_VIDEO_ENCODER_HEVC_CAP_MAX_LEVEL: &str = "HevcMaxLevel";
pub const AMF_VIDEO_ENCODER_HEVC_CAP_NUM_OF_HW_INSTANCES: &str = "HevcNumOfHwInstances";
pub const AMF_VIDEO_ENCODER_AV1_CAP_MAX_PROFILE: &str = "Av1MaxProfile";
pub const AMF_VIDEO_ENCODER_AV1_CAP_MAX_LEVEL: &str = "Av1MaxLevel";
pub const AMF_VIDEO_ENCODER_AV1_CAP_BFRAMES: &str = "AV1BFrames";
pub const AMF_VIDEO_ENCODER_AV1_CAP_NUM_OF_HW_INSTANCES: &str = "Av1CapNumOfHwInstances";
pub const AMF_VIDEO_DECODER_CAP_NUM_OF_HW_INSTANCES: &str = "NumOfHwDecoderInstances";

pub const AMF_VIDEO_ENCODER_PROFILE_BASELINE: i64 = 66;
pub const AMF_VIDEO_ENCODER_PROFILE_MAIN: i64 = 77;
pub const AMF_VIDEO_ENCODER_PROFILE_HIGH: i64 = 100;
pub const AMF_VIDEO_ENCODER_PROFILE_CONSTRAINED_BASELINE: i64 = 256;
pub const AMF_VIDEO_ENCODER_PROFILE_CONSTRAINED_HIGH: i64 = 257;
pub const AMF_VIDEO_ENCODER_HEVC_PROFILE_MAIN: i64 = 1;
pub const AMF_VIDEO_ENCODER_HEVC_PROFILE_MAIN_10: i64 = 2;
pub const AMF_VIDEO_ENCODER_AV1_PROFILE_MAIN: i64 = 1;

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AMFGuid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data41: u8,
    pub data42: u8,
    pub data43: u8,
    pub data44: u8,
    pub data45: u8,
    pub data46: u8,
    pub data47: u8,
    pub data48: u8,
}

pub const IID_AMFContext1: AMFGuid = AMFGuid {
    data1: 0xd9e9f868,
    data2: 0x6220,
    data3: 0x44c6,
    data41: 0xa2,
    data42: 0x2f,
    data43: 0x7c,
    data44: 0xd6,
    data45: 0xda,
    data46: 0xc6,
    data47: 0x86,
    data48: 0x46,
};

/// The widest arms are 16 bytes and the pointer arms force 8-byte alignment; the
/// layout tests depend on both.
#[repr(C)]
#[derive(Copy, Clone)]
pub union AMFVariantValue {
    pub boolValue: amf_bool,
    pub int64Value: amf_int64,
    pub doubleValue: f64,
    pub stringValue: *mut c_char,
    pub wstringValue: *mut amf_wchar,
    pub pInterface: *mut c_void,
    pub rectValue: [i32; 4],
    pub sizeValue: [i32; 2],
    pub pointValue: [i32; 2],
    pub rateValue: [u32; 2],
    pub ratioValue: [u32; 2],
    pub colorValue: u32,
    pub floatValue: f32,
    pub floatSizeValue: [f32; 2],
    pub floatPoint2DValue: [f32; 2],
    pub floatPoint3DValue: [f32; 3],
    pub floatVector4DValue: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct AMFVariantStruct {
    pub r#type: AMF_VARIANT_TYPE,
    pub value: AMFVariantValue,
}

impl AMFVariantStruct {
    pub fn empty() -> Self {
        // all arms are integers, floats, pointers or arrays of those, so zero is valid
        unsafe { std::mem::zeroed() }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self.r#type {
            AMF_VARIANT_BOOL => Some(unsafe { self.value.boolValue } != 0),
            AMF_VARIANT_INT64 => Some(unsafe { self.value.int64Value } != 0),
            _ => None,
        }
    }

    pub fn as_int64(&self) -> Option<i64> {
        match self.r#type {
            AMF_VARIANT_INT64 => Some(unsafe { self.value.int64Value }),
            AMF_VARIANT_BOOL => Some(i64::from(unsafe { self.value.boolValue })),
            _ => None,
        }
    }
}

/// Slot the probe never calls. Any fn pointer keeps the offsets right; the signature
/// only matters for slots invoked from Rust.
pub type AmfVtblFn = unsafe extern "system" fn();

/// Acquire, Release and QueryInterface are the first three slots of every AMF vtable.
pub trait AmfInterface {
    /// # Safety
    /// `this` must point to a live interface obtained from the AMF runtime.
    unsafe fn release(this: *mut Self) -> amf_long;
}

macro_rules! amf_interface {
    ($name:ident, $vtbl:ident) => {
        #[repr(C)]
        pub struct $name {
            pub pVtbl: *const $vtbl,
        }

        impl AmfInterface for $name {
            unsafe fn release(this: *mut Self) -> amf_long {
                unsafe { ((*(*this).pVtbl).Release)(this) }
            }
        }
    };
}

// AMFFactory is not reference counted

#[repr(C)]
pub struct AMFFactory {
    pub pVtbl: *const AMFFactoryVtbl,
}

#[repr(C)]
pub struct AMFFactoryVtbl {
    pub CreateContext:
        unsafe extern "system" fn(*mut AMFFactory, *mut *mut AMFContext) -> AMF_RESULT,
    pub CreateComponent: unsafe extern "system" fn(
        *mut AMFFactory,
        *mut AMFContext,
        *const amf_wchar,
        *mut *mut AMFComponent,
    ) -> AMF_RESULT,
    pub SetCacheFolder: AmfVtblFn,
    pub GetCacheFolder: AmfVtblFn,
    pub GetDebug: AmfVtblFn,
    pub GetTrace: unsafe extern "system" fn(*mut AMFFactory, *mut *mut AMFTrace) -> AMF_RESULT,
    pub GetPrograms: AmfVtblFn,
}

amf_interface!(AMFTrace, AMFTraceVtbl);

#[repr(C)]
pub struct AMFTraceVtbl {
    pub Acquire: unsafe extern "system" fn(*mut AMFTrace) -> amf_long,
    pub Release: unsafe extern "system" fn(*mut AMFTrace) -> amf_long,
    pub QueryInterface: AmfVtblFn,
    pub TraceW: AmfVtblFn,
    pub Trace: AmfVtblFn,
    pub SetGlobalLevel: AmfVtblFn,
    pub GetGlobalLevel: AmfVtblFn,
    pub EnableWriter: AmfVtblFn,
    pub WriterEnabled: AmfVtblFn,
    pub TraceEnableAsync: AmfVtblFn,
    pub TraceFlush: AmfVtblFn,
    pub SetWriterLevel: AmfVtblFn,
    pub GetWriterLevel: AmfVtblFn,
    pub SetWriterLevelForScope: AmfVtblFn,
    pub GetWriterLevelForScope: AmfVtblFn,
    pub GetIndentation: AmfVtblFn,
    pub Indent: AmfVtblFn,
    pub GetPath: AmfVtblFn,
    pub SetPath: AmfVtblFn,
    pub RegisterWriter: AmfVtblFn,
    pub UnregisterWriter: AmfVtblFn,
    pub GetResultText: unsafe extern "system" fn(*mut AMFTrace, AMF_RESULT) -> *const amf_wchar,
    pub SurfaceGetFormatName:
        unsafe extern "system" fn(*mut AMFTrace, AMF_SURFACE_FORMAT) -> *const amf_wchar,
    pub SurfaceGetFormatByName: AmfVtblFn,
    pub GetMemoryTypeName:
        unsafe extern "system" fn(*mut AMFTrace, AMF_MEMORY_TYPE) -> *const amf_wchar,
    pub GetMemoryTypeByName: AmfVtblFn,
    pub GetSampleFormatName: AmfVtblFn,
    pub GetSampleFormatByName: AmfVtblFn,
}

amf_interface!(AMFContext, AMFContextVtbl);

#[repr(C)]
pub struct AMFContextVtbl {
    // AMFInterface
    pub Acquire: unsafe extern "system" fn(*mut AMFContext) -> amf_long,
    pub Release: unsafe extern "system" fn(*mut AMFContext) -> amf_long,
    pub QueryInterface:
        unsafe extern "system" fn(*mut AMFContext, *const AMFGuid, *mut *mut c_void) -> AMF_RESULT,
    // AMFPropertyStorage
    pub SetProperty: AmfVtblFn,
    pub GetProperty: AmfVtblFn,
    pub HasProperty: AmfVtblFn,
    pub GetPropertyCount: AmfVtblFn,
    pub GetPropertyAt: AmfVtblFn,
    pub Clear: AmfVtblFn,
    pub AddTo: AmfVtblFn,
    pub CopyTo: AmfVtblFn,
    pub AddObserver: AmfVtblFn,
    pub RemoveObserver: AmfVtblFn,
    // AMFContext
    pub Terminate: unsafe extern "system" fn(*mut AMFContext) -> AMF_RESULT,
    pub InitDX9: unsafe extern "system" fn(*mut AMFContext, *mut c_void) -> AMF_RESULT,
    pub GetDX9Device: AmfVtblFn,
    pub LockDX9: AmfVtblFn,
    pub UnlockDX9: AmfVtblFn,
    pub InitDX11:
        unsafe extern "system" fn(*mut AMFContext, *mut c_void, AMF_DX_VERSION) -> AMF_RESULT,
    pub GetDX11Device: AmfVtblFn,
    pub LockDX11: AmfVtblFn,
    pub UnlockDX11: AmfVtblFn,
    pub InitOpenCL: AmfVtblFn,
    pub GetOpenCLContext: AmfVtblFn,
    pub GetOpenCLCommandQueue: AmfVtblFn,
    pub GetOpenCLDeviceID: AmfVtblFn,
    pub GetOpenCLComputeFactory: AmfVtblFn,
    pub InitOpenCLEx: AmfVtblFn,
    pub LockOpenCL: AmfVtblFn,
    pub UnlockOpenCL: AmfVtblFn,
    pub InitOpenGL: AmfVtblFn,
    pub GetOpenGLContext: AmfVtblFn,
    pub GetOpenGLDrawable: AmfVtblFn,
    pub LockOpenGL: AmfVtblFn,
    pub UnlockOpenGL: AmfVtblFn,
    pub InitXV: AmfVtblFn,
    pub GetXVDevice: AmfVtblFn,
    pub LockXV: AmfVtblFn,
    pub UnlockXV: AmfVtblFn,
    pub InitGralloc: AmfVtblFn,
    pub GetGrallocDevice: AmfVtblFn,
    pub LockGralloc: AmfVtblFn,
    pub UnlockGralloc: AmfVtblFn,
    pub AllocBuffer: AmfVtblFn,
    pub AllocSurface: AmfVtblFn,
    pub AllocAudioBuffer: AmfVtblFn,
    pub CreateBufferFromHostNative: AmfVtblFn,
    pub CreateSurfaceFromHostNative: AmfVtblFn,
    pub CreateSurfaceFromDX9Native: AmfVtblFn,
    pub CreateSurfaceFromDX11Native: AmfVtblFn,
    pub CreateSurfaceFromOpenGLNative: AmfVtblFn,
    pub CreateSurfaceFromGrallocNative: AmfVtblFn,
    pub CreateSurfaceFromOpenCLNative: AmfVtblFn,
    pub CreateBufferFromOpenCLNative: AmfVtblFn,
    pub GetCompute: AmfVtblFn,
}

amf_interface!(AMFContext1, AMFContext1Vtbl);

/// Adds Vulkan to AMFContext; reached through QueryInterface(IID_AMFContext1).
#[repr(C)]
pub struct AMFContext1Vtbl {
    // AMFInterface
    pub Acquire: unsafe extern "system" fn(*mut AMFContext1) -> amf_long,
    pub Release: unsafe extern "system" fn(*mut AMFContext1) -> amf_long,
    pub QueryInterface: AmfVtblFn,
    // AMFPropertyStorage
    pub SetProperty: AmfVtblFn,
    pub GetProperty: AmfVtblFn,
    pub HasProperty: AmfVtblFn,
    pub GetPropertyCount: AmfVtblFn,
    pub GetPropertyAt: AmfVtblFn,
    pub Clear: AmfVtblFn,
    pub AddTo: AmfVtblFn,
    pub CopyTo: AmfVtblFn,
    pub AddObserver: AmfVtblFn,
    pub RemoveObserver: AmfVtblFn,
    // AMFContext
    pub Terminate: unsafe extern "system" fn(*mut AMFContext1) -> AMF_RESULT,
    pub InitDX9: AmfVtblFn,
    pub GetDX9Device: AmfVtblFn,
    pub LockDX9: AmfVtblFn,
    pub UnlockDX9: AmfVtblFn,
    pub InitDX11: AmfVtblFn,
    pub GetDX11Device: AmfVtblFn,
    pub LockDX11: AmfVtblFn,
    pub UnlockDX11: AmfVtblFn,
    pub InitOpenCL: AmfVtblFn,
    pub GetOpenCLContext: AmfVtblFn,
    pub GetOpenCLCommandQueue: AmfVtblFn,
    pub GetOpenCLDeviceID: AmfVtblFn,
    pub GetOpenCLComputeFactory: AmfVtblFn,
    pub InitOpenCLEx: AmfVtblFn,
    pub LockOpenCL: AmfVtblFn,
    pub UnlockOpenCL: AmfVtblFn,
    pub InitOpenGL: AmfVtblFn,
    pub GetOpenGLContext: AmfVtblFn,
    pub GetOpenGLDrawable: AmfVtblFn,
    pub LockOpenGL: AmfVtblFn,
    pub UnlockOpenGL: AmfVtblFn,
    pub InitXV: AmfVtblFn,
    pub GetXVDevice: AmfVtblFn,
    pub LockXV: AmfVtblFn,
    pub UnlockXV: AmfVtblFn,
    pub InitGralloc: AmfVtblFn,
    pub GetGrallocDevice: AmfVtblFn,
    pub LockGralloc: AmfVtblFn,
    pub UnlockGralloc: AmfVtblFn,
    pub AllocBuffer: AmfVtblFn,
    pub AllocSurface: AmfVtblFn,
    pub AllocAudioBuffer: AmfVtblFn,
    pub CreateBufferFromHostNative: AmfVtblFn,
    pub CreateSurfaceFromHostNative: AmfVtblFn,
    pub CreateSurfaceFromDX9Native: AmfVtblFn,
    pub CreateSurfaceFromDX11Native: AmfVtblFn,
    pub CreateSurfaceFromOpenGLNative: AmfVtblFn,
    pub CreateSurfaceFromGrallocNative: AmfVtblFn,
    pub CreateSurfaceFromOpenCLNative: AmfVtblFn,
    pub CreateBufferFromOpenCLNative: AmfVtblFn,
    pub GetCompute: AmfVtblFn,
    // AMFContext1
    pub CreateBufferFromDX11Native: AmfVtblFn,
    pub AllocBufferEx: AmfVtblFn,
    pub AllocSurfaceEx: AmfVtblFn,
    pub InitVulkan: unsafe extern "system" fn(*mut AMFContext1, *mut c_void) -> AMF_RESULT,
    pub GetVulkanDevice: AmfVtblFn,
    pub LockVulkan: AmfVtblFn,
    pub UnlockVulkan: AmfVtblFn,
    pub CreateSurfaceFromVulkanNative: AmfVtblFn,
    pub CreateBufferFromVulkanNative: AmfVtblFn,
    pub GetVulkanDeviceExtensions: AmfVtblFn,
}

amf_interface!(AMFComponent, AMFComponentVtbl);

#[repr(C)]
pub struct AMFComponentVtbl {
    // AMFInterface
    pub Acquire: unsafe extern "system" fn(*mut AMFComponent) -> amf_long,
    pub Release: unsafe extern "system" fn(*mut AMFComponent) -> amf_long,
    pub QueryInterface: AmfVtblFn,
    // AMFPropertyStorage
    pub SetProperty: unsafe extern "system" fn(
        *mut AMFComponent,
        *const amf_wchar,
        AMFVariantStruct,
    ) -> AMF_RESULT,
    pub GetProperty: unsafe extern "system" fn(
        *mut AMFComponent,
        *const amf_wchar,
        *mut AMFVariantStruct,
    ) -> AMF_RESULT,
    pub HasProperty: AmfVtblFn,
    pub GetPropertyCount: AmfVtblFn,
    pub GetPropertyAt: AmfVtblFn,
    pub Clear: AmfVtblFn,
    pub AddTo: AmfVtblFn,
    pub CopyTo: AmfVtblFn,
    pub AddObserver: AmfVtblFn,
    pub RemoveObserver: AmfVtblFn,
    // AMFPropertyStorageEx
    pub GetPropertiesInfoCount: AmfVtblFn,
    pub GetPropertyInfoAt: AmfVtblFn,
    pub GetPropertyInfo: AmfVtblFn,
    pub ValidateProperty: AmfVtblFn,
    // AMFComponent
    pub Init: unsafe extern "system" fn(
        *mut AMFComponent,
        AMF_SURFACE_FORMAT,
        amf_int32,
        amf_int32,
    ) -> AMF_RESULT,
    pub ReInit: AmfVtblFn,
    pub Terminate: unsafe extern "system" fn(*mut AMFComponent) -> AMF_RESULT,
    pub Drain: AmfVtblFn,
    pub Flush: AmfVtblFn,
    pub SubmitInput: AmfVtblFn,
    pub QueryOutput: AmfVtblFn,
    pub GetContext: AmfVtblFn,
    pub SetOutputDataAllocatorCB: AmfVtblFn,
    pub GetCaps: unsafe extern "system" fn(*mut AMFComponent, *mut *mut AMFCaps) -> AMF_RESULT,
    pub Optimize: AmfVtblFn,
}

amf_interface!(AMFCaps, AMFCapsVtbl);

#[repr(C)]
pub struct AMFCapsVtbl {
    // AMFInterface
    pub Acquire: unsafe extern "system" fn(*mut AMFCaps) -> amf_long,
    pub Release: unsafe extern "system" fn(*mut AMFCaps) -> amf_long,
    pub QueryInterface: AmfVtblFn,
    // AMFPropertyStorage
    pub SetProperty: AmfVtblFn,
    pub GetProperty: unsafe extern "system" fn(
        *mut AMFCaps,
        *const amf_wchar,
        *mut AMFVariantStruct,
    ) -> AMF_RESULT,
    pub HasProperty: unsafe extern "system" fn(*mut AMFCaps, *const amf_wchar) -> amf_bool,
    pub GetPropertyCount: unsafe extern "system" fn(*mut AMFCaps) -> amf_size,
    pub GetPropertyAt: unsafe extern "system" fn(
        *mut AMFCaps,
        amf_size,
        *mut amf_wchar,
        amf_size,
        *mut AMFVariantStruct,
    ) -> AMF_RESULT,
    pub Clear: AmfVtblFn,
    pub AddTo: AmfVtblFn,
    pub CopyTo: AmfVtblFn,
    pub AddObserver: AmfVtblFn,
    pub RemoveObserver: AmfVtblFn,
    // AMFCaps
    pub GetAccelerationType: unsafe extern "system" fn(*mut AMFCaps) -> AMF_ACCELERATION_TYPE,
    pub GetInputCaps: unsafe extern "system" fn(*mut AMFCaps, *mut *mut AMFIOCaps) -> AMF_RESULT,
    pub GetOutputCaps: unsafe extern "system" fn(*mut AMFCaps, *mut *mut AMFIOCaps) -> AMF_RESULT,
}

amf_interface!(AMFIOCaps, AMFIOCapsVtbl);

#[repr(C)]
pub struct AMFIOCapsVtbl {
    // AMFInterface
    pub Acquire: unsafe extern "system" fn(*mut AMFIOCaps) -> amf_long,
    pub Release: unsafe extern "system" fn(*mut AMFIOCaps) -> amf_long,
    pub QueryInterface: AmfVtblFn,
    // AMFIOCaps
    pub GetWidthRange: unsafe extern "system" fn(*mut AMFIOCaps, *mut amf_int32, *mut amf_int32),
    pub GetHeightRange: unsafe extern "system" fn(*mut AMFIOCaps, *mut amf_int32, *mut amf_int32),
    pub GetVertAlign: unsafe extern "system" fn(*mut AMFIOCaps) -> amf_int32,
    pub GetNumOfFormats: unsafe extern "system" fn(*mut AMFIOCaps) -> amf_int32,
    pub GetFormatAt: unsafe extern "system" fn(
        *mut AMFIOCaps,
        amf_int32,
        *mut AMF_SURFACE_FORMAT,
        *mut amf_bool,
    ) -> AMF_RESULT,
    pub GetNumOfMemoryTypes: unsafe extern "system" fn(*mut AMFIOCaps) -> amf_int32,
    pub GetMemoryTypeAt: unsafe extern "system" fn(
        *mut AMFIOCaps,
        amf_int32,
        *mut AMF_MEMORY_TYPE,
        *mut amf_bool,
    ) -> AMF_RESULT,
    pub IsInterlacedSupported: unsafe extern "system" fn(*mut AMFIOCaps) -> amf_bool,
}

#[cfg(test)]
mod layout_tests {
    use std::mem::{align_of, offset_of, size_of};

    use super::*;

    const PTR: usize = size_of::<*const c_void>();

    #[test]
    fn guid_matches_header() {
        assert_eq!(size_of::<AMFGuid>(), 16);
        assert_eq!(align_of::<AMFGuid>(), 4);
        assert_eq!(offset_of!(AMFGuid, data41), 8);
    }

    #[test]
    fn variant_matches_header() {
        assert_eq!(size_of::<AMFVariantValue>(), 16);
        assert_eq!(align_of::<AMFVariantValue>(), 8);
        assert_eq!(size_of::<AMFVariantStruct>(), 24);
        assert_eq!(offset_of!(AMFVariantStruct, value), 8);
    }

    #[test]
    fn vtables_have_expected_slot_counts() {
        assert_eq!(size_of::<AMFFactoryVtbl>(), 7 * PTR);
        assert_eq!(size_of::<AMFTraceVtbl>(), 28 * PTR);
        assert_eq!(size_of::<AMFContextVtbl>(), 55 * PTR);
        assert_eq!(size_of::<AMFContext1Vtbl>(), 65 * PTR);
        assert_eq!(size_of::<AMFComponentVtbl>(), 28 * PTR);
        assert_eq!(size_of::<AMFCapsVtbl>(), 16 * PTR);
        assert_eq!(size_of::<AMFIOCapsVtbl>(), 11 * PTR);
    }

    #[test]
    fn used_slots_are_at_header_offsets() {
        assert_eq!(offset_of!(AMFContextVtbl, Terminate), 13 * PTR);
        assert_eq!(offset_of!(AMFContextVtbl, InitDX9), 14 * PTR);
        assert_eq!(offset_of!(AMFContextVtbl, InitDX11), 18 * PTR);
        assert_eq!(offset_of!(AMFContext1Vtbl, InitVulkan), 58 * PTR);
        assert_eq!(offset_of!(AMFComponentVtbl, Init), 17 * PTR);
        assert_eq!(offset_of!(AMFComponentVtbl, GetCaps), 26 * PTR);
        assert_eq!(offset_of!(AMFCapsVtbl, GetAccelerationType), 13 * PTR);
        assert_eq!(offset_of!(AMFIOCapsVtbl, GetFormatAt), 7 * PTR);
        assert_eq!(offset_of!(AMFTraceVtbl, GetResultText), 21 * PTR);
    }

    #[test]
    fn version_round_trips() {
        assert_eq!(amf_version_parts(AMF_FULL_VERSION), (1, 5, 2, 0));
        assert_eq!(
            amf_make_full_version(1, 4, 35, 0),
            0x0001_0004_0023_0000_u64
        );
    }

    #[test]
    fn wide_is_nul_terminated() {
        let w = wide("NV12");
        assert_eq!(w.len(), 5);
        assert_eq!(w[4], 0);
        assert_eq!(unsafe { from_wide(w.as_ptr()) }, "NV12");
    }
}

#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod ffi;

#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub use ffi::*;
