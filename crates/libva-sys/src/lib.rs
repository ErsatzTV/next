use std::ffi::{c_int, c_uint, c_void};

pub type VADisplay = *mut c_void;
pub type VAStatus = c_int;
pub type VAProfile = c_int;
pub type VAEntrypoint = c_int;
pub type VAConfigID = c_uint;
pub type VAGenericValueType = c_int;

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

pub const VA_GENERIC_VALUE_TYPE_INTEGER: VAGenericValueType = 1;

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

#[cfg(target_os = "linux")]
mod ffi;

#[cfg(target_os = "linux")]
pub use ffi::*;
