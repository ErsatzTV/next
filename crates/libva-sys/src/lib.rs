use std::collections::HashMap;
use std::ffi::{c_int, c_uint, c_ushort, c_void};
use std::sync::LazyLock;

pub type VADisplay = *mut c_void;
pub type VAStatus = c_int;
pub type VAProfile = c_int;
pub type VAEntrypoint = c_int;
pub type VAConfigID = c_uint;
pub type VAGenericValueType = c_int;
pub type VASurfaceID = c_uint;
pub type VAContextID = c_uint;
// https://intel.github.io/libva/group__api__vpp.html#ga3614dbee76b8ac89dd5a3dc8b1a12bb7
pub type VAProcFilterType = c_int;

pub const VA_PROFILE_NONE: VAProfile = -1;
pub const VA_PROFILE_MPEG2_SIMPLE: VAProfile = 0;
pub const VA_PROFILE_MPEG2_MAIN: VAProfile = 1;
pub const VA_PROFILE_MPEG4_SIMPLE: VAProfile = 2;
pub const VA_PROFILE_MPEG4_ADVANCED_SIMPLE: VAProfile = 3;
pub const VA_PROFILE_MPEG4_MAIN: VAProfile = 4;
pub const VA_PROFILE_H264_MAIN: VAProfile = 6;
pub const VA_PROFILE_H264_HIGH: VAProfile = 7;
pub const VA_PROFILE_VC1_SIMPLE: VAProfile = 8;
pub const VA_PROFILE_VC1_MAIN: VAProfile = 9;
pub const VA_PROFILE_VC1_ADVANCED: VAProfile = 10;
pub const VA_PROFILE_H264_CONSTRAINED_BASELINE: VAProfile = 13;
pub const VA_PROFILE_HEVC_MAIN: VAProfile = 17;
pub const VA_PROFILE_HEVC_MAIN10: VAProfile = 18;
pub const VA_PROFILE_VP9_PROFILE0: VAProfile = 19;
pub const VA_PROFILE_VP9_PROFILE1: VAProfile = 20;
pub const VA_PROFILE_VP9_PROFILE2: VAProfile = 21;
pub const VA_PROFILE_VP9_PROFILE3: VAProfile = 22;
pub const VA_PROFILE_AV1_PROFILE0: VAProfile = 32;
pub const VA_PROFILE_AV1_PROFILE1: VAProfile = 33;
pub const VA_PROFILE_H264_HIGH10: VAProfile = 36;

pub const VA_ENTRYPOINT_VLD: VAEntrypoint = 1;
pub const VA_ENTRYPOINT_ENC_SLICE: VAEntrypoint = 6;
pub const VA_ENTRYPOINT_ENC_SLICE_LP: VAEntrypoint = 8;
pub const VA_ENTRYPOINT_VIDEO_PROC: VAEntrypoint = 10;

pub const VA_SURFACE_ATTRIB_PIXEL_FORMAT: c_int = 1;
pub const VA_SURFACE_ATTRIB_GETTABLE: c_int = 0x00000001;

pub const VA_FOURCC_NV12: u32 = 0x3231564E; // 'N', 'V', '1', '2'
pub const VA_FOURCC_P010: u32 = 0x30313050; // 'P', '0', '1', '0'

pub const VA_RT_FORMAT_YUV420: c_uint = 0x00000001;
pub const VA_RT_FORMAT_YUV420_10: c_uint = 0x00000100;

pub static VA_FORMAT_MAPPING: LazyLock<HashMap<c_uint, c_uint>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    map.insert(VA_FOURCC_NV12, VA_RT_FORMAT_YUV420);
    map.insert(VA_FOURCC_P010, VA_RT_FORMAT_YUV420_10);
    map
});

pub const VA_PROGRESSIVE: c_int = 0x1;

// HDR cap flags
pub const VA_TONE_MAPPING_HDR_TO_HDR: c_int = 0x0001;
pub const VA_TONE_MAPPING_HDR_TO_SDR: c_int = 0x0002;

pub const VA_GENERIC_VALUE_TYPE_INTEGER: VAGenericValueType = 1;

pub const VA_PROC_FILTER_NONE: VAProcFilterType = 0;
pub const VA_PROC_FILTER_NOISE_REDUCTION: VAProcFilterType = 1;
pub const VA_PROC_FILTER_DEINTERLACING: VAProcFilterType = 2;
pub const VA_PROC_FILTER_SHARPENING: VAProcFilterType = 3;
pub const VA_PROC_FILTER_COLOR_BALANCE: VAProcFilterType = 4;
pub const VA_PROC_FILTER_SKIN_TONE_ENHANCEMENT: VAProcFilterType = 5;
pub const VA_PROC_FILTER_TOTAL_COLOR_CORRECTION: VAProcFilterType = 6;
pub const VA_PROC_FILTER_HVS_NOISE_REDUCTION: VAProcFilterType = 7;
pub const VA_PROC_FILTER_HIGH_DYNAMIC_RANGE_MAPPING: VAProcFilterType = 8;
pub const VA_PROC_FILTER_3D_LUT: VAProcFilterType = 9;
pub const VA_PROC_FILTER_COUNT: VAProcFilterType = 10;

pub const VA_STATUS_SUCCESS: VAStatus = 0;

#[repr(C)]
#[derive(Clone, Copy)]
pub union VAGenericValueValue {
    pub i: i32,
    pub f: f32,
    pub p: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VAGenericValue {
    pub value_type: VAGenericValueType,
    pub value: VAGenericValueValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VASurfaceAttrib {
    pub type_: c_int,
    pub flags: u32,
    pub value: VAGenericValue,
}

pub type VAProcHighDynamicRangeMetadataType = c_int;

pub const VA_PROC_HIGH_DYNAMIC_RANGE_METADATA_NONE: VAProcHighDynamicRangeMetadataType = 0;
pub const VA_PROC_HIGH_DYNAMIC_RANGE_METADATA_HDR10: VAProcHighDynamicRangeMetadataType = 1;
pub const VA_PROC_HIGH_DYNAMIC_RANGE_METADATA_COUNT: VAProcHighDynamicRangeMetadataType = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, derive_more::Display)]
#[display(
    "VAProcFilterCapHighDynamicRange {{ metadata_type: {:?}, caps_flag: {:?} }}",
    metadata_type,
    caps_flag
)]
pub struct VAProcFilterCapHighDynamicRange {
    pub metadata_type: VAProcHighDynamicRangeMetadataType,
    pub caps_flag: c_ushort,
    va_reserved: [c_ushort; 16],
}

#[cfg(target_os = "linux")]
mod ffi;

#[cfg(target_os = "linux")]
pub use ffi::*;
