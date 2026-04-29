#![allow(non_upper_case_globals)]

use std::ffi::{c_char, c_void};

pub type VkInstance = *mut c_void;
pub type VkPhysicalDevice = *mut c_void;
pub type VkResult = i32;

pub const VK_SUCCESS: VkResult = 0;

// Physical device types
pub const VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU: u32 = 2;

// Structure types
pub const VK_STRUCTURE_TYPE_APPLICATION_INFO: u32 = 0;
pub const VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO: u32 = 1;
pub const VK_STRUCTURE_TYPE_VIDEO_PROFILE_INFO_KHR: u32 = 1000023000;
pub const VK_STRUCTURE_TYPE_VIDEO_CAPABILITIES_KHR: u32 = 1000023001;
pub const VK_STRUCTURE_TYPE_VIDEO_DECODE_CAPABILITIES_KHR: u32 = 1000024001;
pub const VK_STRUCTURE_TYPE_VIDEO_ENCODE_H264_CAPABILITIES_KHR: u32 = 1000038000;
pub const VK_STRUCTURE_TYPE_VIDEO_ENCODE_H265_CAPABILITIES_KHR: u32 = 1000039000;
pub const VK_STRUCTURE_TYPE_VIDEO_DECODE_H264_CAPABILITIES_KHR: u32 = 1000040000;
pub const VK_STRUCTURE_TYPE_VIDEO_DECODE_H265_CAPABILITIES_KHR: u32 = 1000187000;
pub const VK_STRUCTURE_TYPE_VIDEO_ENCODE_CAPABILITIES_KHR: u32 = 1000299003;
pub const VK_STRUCTURE_TYPE_VIDEO_DECODE_AV1_CAPABILITIES_KHR: u32 = 1000512000;
pub const VK_STRUCTURE_TYPE_VIDEO_ENCODE_AV1_CAPABILITIES_KHR: u32 = 1000513000;

// Video codec operation flags
pub const VK_VIDEO_CODEC_OPERATION_DECODE_H264_BIT_KHR: u32 = 0x00000001;
pub const VK_VIDEO_CODEC_OPERATION_DECODE_H265_BIT_KHR: u32 = 0x00000002;
pub const VK_VIDEO_CODEC_OPERATION_DECODE_AV1_BIT_KHR: u32 = 0x00000004;
pub const VK_VIDEO_CODEC_OPERATION_ENCODE_H264_BIT_KHR: u32 = 0x00010000;
pub const VK_VIDEO_CODEC_OPERATION_ENCODE_H265_BIT_KHR: u32 = 0x00020000;
pub const VK_VIDEO_CODEC_OPERATION_ENCODE_AV1_BIT_KHR: u32 = 0x00040000;

// Chroma subsampling
pub const VK_VIDEO_CHROMA_SUBSAMPLING_420_BIT_KHR: u32 = 0x02;

// Component bit depth
pub const VK_VIDEO_COMPONENT_BIT_DEPTH_8_BIT_KHR: u32 = 0x01;
pub const VK_VIDEO_COMPONENT_BIT_DEPTH_10_BIT_KHR: u32 = 0x04;

// Codec-specific profile info structure types
pub const VK_STRUCTURE_TYPE_VIDEO_DECODE_H264_PROFILE_INFO_KHR: u32 = 1000040003;
pub const VK_STRUCTURE_TYPE_VIDEO_DECODE_H265_PROFILE_INFO_KHR: u32 = 1000187003;
pub const VK_STRUCTURE_TYPE_VIDEO_DECODE_AV1_PROFILE_INFO_KHR: u32 = 1000512003;
pub const VK_STRUCTURE_TYPE_VIDEO_ENCODE_H264_PROFILE_INFO_KHR: u32 = 1000038007;
pub const VK_STRUCTURE_TYPE_VIDEO_ENCODE_H265_PROFILE_INFO_KHR: u32 = 1000039007;
pub const VK_STRUCTURE_TYPE_VIDEO_ENCODE_AV1_PROFILE_INFO_KHR: u32 = 1000513005;

// H.264 profile IDCs (StdVideoH264ProfileIdc)
pub const STD_VIDEO_H264_PROFILE_IDC_HIGH: u32 = 100;

// H.265 profile IDCs (StdVideoH265ProfileIdc)
pub const STD_VIDEO_H265_PROFILE_IDC_MAIN: u32 = 1;
pub const STD_VIDEO_H265_PROFILE_IDC_MAIN_10: u32 = 2;

// AV1 profiles (StdVideoAV1Profile)
pub const STD_VIDEO_AV1_PROFILE_MAIN: u32 = 0;

// Extension names
pub const VK_KHR_VIDEO_DECODE_H264_EXTENSION_NAME: &str = "VK_KHR_video_decode_h264";
pub const VK_KHR_VIDEO_DECODE_H265_EXTENSION_NAME: &str = "VK_KHR_video_decode_h265";
pub const VK_KHR_VIDEO_DECODE_AV1_EXTENSION_NAME: &str = "VK_KHR_video_decode_av1";
pub const VK_KHR_VIDEO_ENCODE_H264_EXTENSION_NAME: &str = "VK_KHR_video_encode_h264";
pub const VK_KHR_VIDEO_ENCODE_H265_EXTENSION_NAME: &str = "VK_KHR_video_encode_h265";
pub const VK_KHR_VIDEO_ENCODE_AV1_EXTENSION_NAME: &str = "VK_KHR_video_encode_av1";

pub const fn vk_make_api_version(variant: u32, major: u32, minor: u32, patch: u32) -> u32 {
    (variant << 29) | (major << 22) | (minor << 12) | patch
}

#[repr(C)]
pub struct VkApplicationInfo {
    pub s_type: u32,
    pub p_next: *const c_void,
    pub p_application_name: *const c_char,
    pub application_version: u32,
    pub p_engine_name: *const c_char,
    pub engine_version: u32,
    pub api_version: u32,
}

#[repr(C)]
pub struct VkInstanceCreateInfo {
    pub s_type: u32,
    pub p_next: *const c_void,
    pub flags: u32,
    pub p_application_info: *const VkApplicationInfo,
    pub enabled_layer_count: u32,
    pub pp_enabled_layer_names: *const *const c_char,
    pub enabled_extension_count: u32,
    pub pp_enabled_extension_names: *const *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkExtensionProperties {
    pub extension_name: [c_char; 256],
    pub spec_version: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkExtent2D {
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
pub struct VkPhysicalDeviceProperties {
    pub api_version: u32,
    pub driver_version: u32,
    pub vendor_id: u32,
    pub device_id: u32,
    pub device_type: u32,
    pub device_name: [c_char; 256],
    pub pipeline_cache_uuid: [u8; 16],
    pub limits: VkPhysicalDeviceLimits,
    pub sparse_properties: VkPhysicalDeviceSparseProperties,
}

#[repr(C)]
pub struct VkPhysicalDeviceLimits {
    _data: [u8; 504],
}

#[repr(C)]
pub struct VkPhysicalDeviceSparseProperties {
    _data: [u8; 20],
}

#[repr(C)]
pub struct VkVideoDecodeH264ProfileInfoKHR {
    pub s_type: u32,
    pub p_next: *const c_void,
    pub std_profile_idc: u32,
    pub picture_layout: u32,
}

#[repr(C)]
pub struct VkVideoDecodeH265ProfileInfoKHR {
    pub s_type: u32,
    pub p_next: *const c_void,
    pub std_profile_idc: u32,
}

#[repr(C)]
pub struct VkVideoDecodeAV1ProfileInfoKHR {
    pub s_type: u32,
    pub p_next: *const c_void,
    pub std_profile: u32,
    pub film_grain_support: u32,
}

#[repr(C)]
pub struct VkVideoEncodeH264ProfileInfoKHR {
    pub s_type: u32,
    pub p_next: *const c_void,
    pub std_profile_idc: u32,
}

#[repr(C)]
pub struct VkVideoEncodeH265ProfileInfoKHR {
    pub s_type: u32,
    pub p_next: *const c_void,
    pub std_profile_idc: u32,
}

#[repr(C)]
pub struct VkVideoEncodeAV1ProfileInfoKHR {
    pub s_type: u32,
    pub p_next: *const c_void,
    pub std_profile: u32,
}

#[repr(C)]
pub struct VkVideoProfileInfoKHR {
    pub s_type: u32,
    pub p_next: *const c_void,
    pub video_codec_operation: u32,
    pub chroma_subsampling: u32,
    pub luma_bit_depth: u32,
    pub chroma_bit_depth: u32,
}

#[repr(C)]
pub struct VkVideoCapabilitiesKHR {
    pub s_type: u32,
    pub p_next: *mut c_void,
    pub flags: u32,
    pub min_bitstream_buffer_offset_alignment: u64,
    pub min_bitstream_buffer_size_alignment: u64,
    pub picture_access_granularity: VkExtent2D,
    pub min_coded_extent: VkExtent2D,
    pub max_coded_extent: VkExtent2D,
    pub max_dpb_slots: u32,
    pub max_active_reference_pictures: u32,
    pub std_header_version: VkExtensionProperties,
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
