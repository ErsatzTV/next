// Vulkan Video specification: https://registry.khronos.org/vulkan/specs/latest/man/html/VK_KHR_video_queue.html
// Codec-specific extensions: VK_KHR_video_decode_h264, VK_KHR_video_decode_h265,
//   VK_KHR_video_decode_av1, VK_KHR_video_encode_h264, VK_KHR_video_encode_h265,
//   VK_KHR_video_encode_av1

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, c_void};
use std::ptr;

use libvulkan_sys::{
    STD_VIDEO_AV1_PROFILE_MAIN, STD_VIDEO_H264_PROFILE_IDC_HIGH, STD_VIDEO_H265_PROFILE_IDC_MAIN,
    STD_VIDEO_H265_PROFILE_IDC_MAIN_10, VK_KHR_VIDEO_DECODE_AV1_EXTENSION_NAME,
    VK_KHR_VIDEO_DECODE_H264_EXTENSION_NAME, VK_KHR_VIDEO_DECODE_H265_EXTENSION_NAME,
    VK_KHR_VIDEO_ENCODE_AV1_EXTENSION_NAME, VK_KHR_VIDEO_ENCODE_H264_EXTENSION_NAME,
    VK_KHR_VIDEO_ENCODE_H265_EXTENSION_NAME, VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU,
    VK_STRUCTURE_TYPE_APPLICATION_INFO, VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
    VK_STRUCTURE_TYPE_VIDEO_CAPABILITIES_KHR, VK_STRUCTURE_TYPE_VIDEO_DECODE_AV1_CAPABILITIES_KHR,
    VK_STRUCTURE_TYPE_VIDEO_DECODE_AV1_PROFILE_INFO_KHR,
    VK_STRUCTURE_TYPE_VIDEO_DECODE_CAPABILITIES_KHR,
    VK_STRUCTURE_TYPE_VIDEO_DECODE_H264_CAPABILITIES_KHR,
    VK_STRUCTURE_TYPE_VIDEO_DECODE_H264_PROFILE_INFO_KHR,
    VK_STRUCTURE_TYPE_VIDEO_DECODE_H265_CAPABILITIES_KHR,
    VK_STRUCTURE_TYPE_VIDEO_DECODE_H265_PROFILE_INFO_KHR,
    VK_STRUCTURE_TYPE_VIDEO_ENCODE_AV1_CAPABILITIES_KHR,
    VK_STRUCTURE_TYPE_VIDEO_ENCODE_AV1_PROFILE_INFO_KHR,
    VK_STRUCTURE_TYPE_VIDEO_ENCODE_CAPABILITIES_KHR,
    VK_STRUCTURE_TYPE_VIDEO_ENCODE_H264_CAPABILITIES_KHR,
    VK_STRUCTURE_TYPE_VIDEO_ENCODE_H264_PROFILE_INFO_KHR,
    VK_STRUCTURE_TYPE_VIDEO_ENCODE_H265_CAPABILITIES_KHR,
    VK_STRUCTURE_TYPE_VIDEO_ENCODE_H265_PROFILE_INFO_KHR, VK_STRUCTURE_TYPE_VIDEO_PROFILE_INFO_KHR,
    VK_SUCCESS, VK_VIDEO_CHROMA_SUBSAMPLING_420_BIT_KHR,
    VK_VIDEO_CODEC_OPERATION_DECODE_AV1_BIT_KHR, VK_VIDEO_CODEC_OPERATION_DECODE_H264_BIT_KHR,
    VK_VIDEO_CODEC_OPERATION_DECODE_H265_BIT_KHR, VK_VIDEO_CODEC_OPERATION_ENCODE_AV1_BIT_KHR,
    VK_VIDEO_CODEC_OPERATION_ENCODE_H264_BIT_KHR, VK_VIDEO_CODEC_OPERATION_ENCODE_H265_BIT_KHR,
    VK_VIDEO_COMPONENT_BIT_DEPTH_8_BIT_KHR, VK_VIDEO_COMPONENT_BIT_DEPTH_10_BIT_KHR,
    VkApplicationInfo, VkExtensionProperties, VkInstanceCreateInfo, VkLib, VkPhysicalDevice,
    VkPhysicalDeviceProperties, VkResult, VkVideoCapabilitiesKHR, VkVideoDecodeAV1ProfileInfoKHR,
    VkVideoDecodeH264ProfileInfoKHR, VkVideoDecodeH265ProfileInfoKHR,
    VkVideoEncodeAV1ProfileInfoKHR, VkVideoEncodeH264ProfileInfoKHR,
    VkVideoEncodeH265ProfileInfoKHR, VkVideoProfileInfoKHR, vk_make_api_version,
};

use crate::capabilities::vulkan::VulkanCapabilities;
use crate::error::FFPipelineError;
use crate::pipeline::VideoFormat;

type PfnGetVideoCapabilities = unsafe extern "C" fn(
    VkPhysicalDevice,
    *const VkVideoProfileInfoKHR,
    *mut VkVideoCapabilitiesKHR,
) -> VkResult;

const VIDEO_EXTENSIONS: &[&str] = &[
    VK_KHR_VIDEO_DECODE_H264_EXTENSION_NAME,
    VK_KHR_VIDEO_DECODE_H265_EXTENSION_NAME,
    VK_KHR_VIDEO_DECODE_AV1_EXTENSION_NAME,
    VK_KHR_VIDEO_ENCODE_H264_EXTENSION_NAME,
    VK_KHR_VIDEO_ENCODE_H265_EXTENSION_NAME,
    VK_KHR_VIDEO_ENCODE_AV1_EXTENSION_NAME,
];

const BIT_DEPTHS: &[(u32, u8)] = &[
    (VK_VIDEO_COMPONENT_BIT_DEPTH_8_BIT_KHR, 8),
    (VK_VIDEO_COMPONENT_BIT_DEPTH_10_BIT_KHR, 10),
];

/// Opaque buffer large enough to hold any VkVideo*CapabilitiesKHR struct.
/// We only need sType and pNext to satisfy Vulkan valid usage; the driver
/// fills in the rest but we never read it.
#[repr(C, align(8))]
struct CapsBuf {
    s_type: u32,
    _pad: u32,
    p_next: *mut c_void,
    _data: [u8; 240],
}

impl CapsBuf {
    fn new(s_type: u32) -> Self {
        Self {
            s_type,
            _pad: 0,
            p_next: ptr::null_mut(),
            _data: [0u8; 240],
        }
    }
}

impl VulkanCapabilities {
    pub fn probe() -> Result<VulkanCapabilities, FFPipelineError> {
        let vk = VkLib::load().map_err(|e| {
            FFPipelineError::VulkanCapabilitiesError(format!("failed to load libvulkan: {e}"))
        })?;

        unsafe { probe_vulkan(&vk) }
    }
}

unsafe fn probe_vulkan(vk: &VkLib) -> Result<VulkanCapabilities, FFPipelineError> {
    unsafe {
        let app_name = b"ersatztv\0";
        let app_info = VkApplicationInfo {
            s_type: VK_STRUCTURE_TYPE_APPLICATION_INFO,
            p_next: ptr::null(),
            p_application_name: app_name.as_ptr().cast(),
            application_version: 0,
            p_engine_name: ptr::null(),
            engine_version: 0,
            api_version: vk_make_api_version(0, 1, 3, 0),
        };

        let create_info = VkInstanceCreateInfo {
            s_type: VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            p_application_info: &app_info,
            enabled_layer_count: 0,
            pp_enabled_layer_names: ptr::null(),
            enabled_extension_count: 0,
            pp_enabled_extension_names: ptr::null(),
        };

        let mut instance = ptr::null_mut();
        let result = (vk.vkCreateInstance)(&create_info, ptr::null(), &mut instance);
        if result != VK_SUCCESS {
            return Err(FFPipelineError::VulkanCapabilitiesError(format!(
                "vkCreateInstance failed: {result}"
            )));
        }

        let caps = probe_with_instance(vk, instance);

        (vk.vkDestroyInstance)(instance, ptr::null());

        caps
    }
}

unsafe fn get_device_name(vk: &VkLib, device: VkPhysicalDevice) -> String {
    unsafe {
        let mut props: VkPhysicalDeviceProperties = std::mem::zeroed();
        (vk.vkGetPhysicalDeviceProperties)(device, &mut props);
        CStr::from_ptr(props.device_name.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}

unsafe fn get_device_type(vk: &VkLib, device: VkPhysicalDevice) -> u32 {
    unsafe {
        let mut props: VkPhysicalDeviceProperties = std::mem::zeroed();
        (vk.vkGetPhysicalDeviceProperties)(device, &mut props);
        props.device_type
    }
}

fn device_type_name(device_type: u32) -> &'static str {
    match device_type {
        0 => "other",
        1 => "integrated",
        2 => "discrete",
        3 => "virtual",
        4 => "cpu",
        _ => "unknown",
    }
}

unsafe fn probe_with_instance(
    vk: &VkLib,
    instance: libvulkan_sys::VkInstance,
) -> Result<VulkanCapabilities, FFPipelineError> {
    unsafe {
        let mut device_count: u32 = 0;
        (vk.vkEnumeratePhysicalDevices)(instance, &mut device_count, ptr::null_mut());
        if device_count == 0 {
            return Err(FFPipelineError::VulkanCapabilitiesError(
                "no Vulkan physical devices found".into(),
            ));
        }

        let mut devices = vec![ptr::null_mut(); device_count as usize];
        (vk.vkEnumeratePhysicalDevices)(instance, &mut device_count, devices.as_mut_ptr());

        log::debug!("[vulkan] found {} physical device(s)", device_count);

        let mut best_device = None;
        let mut best_score: i32 = -1;

        for (i, &device) in devices.iter().enumerate() {
            let name = get_device_name(vk, device);
            let device_type = get_device_type(vk, device);
            let ext_names = enumerate_device_extensions(vk, device)?;
            let video_ext_count = VIDEO_EXTENSIONS
                .iter()
                .filter(|e| ext_names.contains(**e))
                .count();

            log::debug!(
                "[vulkan]   device {}: \"{}\" ({}), {} video extension(s)",
                i,
                name,
                device_type_name(device_type),
                video_ext_count,
            );

            for ext in VIDEO_EXTENSIONS {
                if ext_names.contains(*ext) {
                    log::debug!("[vulkan]     {}", ext);
                }
            }

            let mut score = video_ext_count as i32;
            if device_type == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU {
                score += 100;
            }

            if score > best_score {
                best_score = score;
                best_device = Some((i as u32, device, name, ext_names));
            }
        }

        let (device_index, physical_device, device_name, ext_names) = best_device.unwrap();
        log::debug!(
            "[vulkan] selected device {}: \"{}\"",
            device_index,
            device_name
        );

        let get_video_caps = (vk.vkGetInstanceProcAddr)(
            instance,
            c"vkGetPhysicalDeviceVideoCapabilitiesKHR".as_ptr(),
        );

        let mut supported_decoders = HashMap::new();
        let mut supported_encoders = HashMap::new();

        if let Some(fn_ptr) = get_video_caps {
            let get_video_caps: PfnGetVideoCapabilities = std::mem::transmute(fn_ptr);

            probe_decoders(
                physical_device,
                &ext_names,
                get_video_caps,
                &mut supported_decoders,
            );
            probe_encoders(
                physical_device,
                &ext_names,
                get_video_caps,
                &mut supported_encoders,
            );
        } else {
            log::warn!(
                "[vulkan] vkGetPhysicalDeviceVideoCapabilitiesKHR not available; \
                 falling back to extension-presence detection"
            );
            for &(format, ext_name) in &[
                (VideoFormat::H264, VK_KHR_VIDEO_DECODE_H264_EXTENSION_NAME),
                (VideoFormat::Hevc, VK_KHR_VIDEO_DECODE_H265_EXTENSION_NAME),
                (VideoFormat::Av1, VK_KHR_VIDEO_DECODE_AV1_EXTENSION_NAME),
            ] {
                if ext_names.contains(ext_name) {
                    supported_decoders.insert(format, vec![8]);
                }
            }
            for &(format, ext_name) in &[
                (VideoFormat::H264, VK_KHR_VIDEO_ENCODE_H264_EXTENSION_NAME),
                (VideoFormat::Hevc, VK_KHR_VIDEO_ENCODE_H265_EXTENSION_NAME),
                (VideoFormat::Av1, VK_KHR_VIDEO_ENCODE_AV1_EXTENSION_NAME),
            ] {
                if ext_names.contains(ext_name) {
                    supported_encoders.insert(format, vec![8]);
                }
            }
        }

        Ok(VulkanCapabilities {
            device_index,
            supported_decoders,
            supported_encoders,
        })
    }
}

unsafe fn probe_decoders(
    device: VkPhysicalDevice,
    ext_names: &HashSet<String>,
    get_video_caps: PfnGetVideoCapabilities,
    out: &mut HashMap<VideoFormat, Vec<u8>>,
) {
    unsafe {
        // H.264: same profile (High) for all bit depths
        if ext_names.contains(VK_KHR_VIDEO_DECODE_H264_EXTENSION_NAME) {
            let profile_info = VkVideoDecodeH264ProfileInfoKHR {
                s_type: VK_STRUCTURE_TYPE_VIDEO_DECODE_H264_PROFILE_INFO_KHR,
                p_next: ptr::null(),
                std_profile_idc: STD_VIDEO_H264_PROFILE_IDC_HIGH,
                picture_layout: 0,
            };
            let depths = probe_bit_depths(
                device,
                VK_VIDEO_CODEC_OPERATION_DECODE_H264_BIT_KHR,
                get_video_caps,
                &profile_info as *const _ as *const c_void,
                VK_STRUCTURE_TYPE_VIDEO_DECODE_CAPABILITIES_KHR,
                VK_STRUCTURE_TYPE_VIDEO_DECODE_H264_CAPABILITIES_KHR,
            );
            if !depths.is_empty() {
                out.insert(VideoFormat::H264, depths);
            }
        }

        // H.265: different profile per bit depth (Main vs Main 10)
        if ext_names.contains(VK_KHR_VIDEO_DECODE_H265_EXTENSION_NAME) {
            let mut depths = Vec::new();

            let profile_8 = VkVideoDecodeH265ProfileInfoKHR {
                s_type: VK_STRUCTURE_TYPE_VIDEO_DECODE_H265_PROFILE_INFO_KHR,
                p_next: ptr::null(),
                std_profile_idc: STD_VIDEO_H265_PROFILE_IDC_MAIN,
            };
            if try_profile(
                device,
                VK_VIDEO_CODEC_OPERATION_DECODE_H265_BIT_KHR,
                VK_VIDEO_COMPONENT_BIT_DEPTH_8_BIT_KHR,
                &profile_8 as *const _ as *const c_void,
                VK_STRUCTURE_TYPE_VIDEO_DECODE_CAPABILITIES_KHR,
                VK_STRUCTURE_TYPE_VIDEO_DECODE_H265_CAPABILITIES_KHR,
                get_video_caps,
            ) {
                depths.push(8);
            }

            let profile_10 = VkVideoDecodeH265ProfileInfoKHR {
                s_type: VK_STRUCTURE_TYPE_VIDEO_DECODE_H265_PROFILE_INFO_KHR,
                p_next: ptr::null(),
                std_profile_idc: STD_VIDEO_H265_PROFILE_IDC_MAIN_10,
            };
            if try_profile(
                device,
                VK_VIDEO_CODEC_OPERATION_DECODE_H265_BIT_KHR,
                VK_VIDEO_COMPONENT_BIT_DEPTH_10_BIT_KHR,
                &profile_10 as *const _ as *const c_void,
                VK_STRUCTURE_TYPE_VIDEO_DECODE_CAPABILITIES_KHR,
                VK_STRUCTURE_TYPE_VIDEO_DECODE_H265_CAPABILITIES_KHR,
                get_video_caps,
            ) {
                depths.push(10);
            }

            if !depths.is_empty() {
                out.insert(VideoFormat::Hevc, depths);
            }
        }

        // AV1: Main profile supports both 8 and 10 bit
        if ext_names.contains(VK_KHR_VIDEO_DECODE_AV1_EXTENSION_NAME) {
            let profile_info = VkVideoDecodeAV1ProfileInfoKHR {
                s_type: VK_STRUCTURE_TYPE_VIDEO_DECODE_AV1_PROFILE_INFO_KHR,
                p_next: ptr::null(),
                std_profile: STD_VIDEO_AV1_PROFILE_MAIN,
                film_grain_support: 0,
            };
            let depths = probe_bit_depths(
                device,
                VK_VIDEO_CODEC_OPERATION_DECODE_AV1_BIT_KHR,
                get_video_caps,
                &profile_info as *const _ as *const c_void,
                VK_STRUCTURE_TYPE_VIDEO_DECODE_CAPABILITIES_KHR,
                VK_STRUCTURE_TYPE_VIDEO_DECODE_AV1_CAPABILITIES_KHR,
            );
            if !depths.is_empty() {
                out.insert(VideoFormat::Av1, depths);
            }
        }
    }
}

unsafe fn probe_encoders(
    device: VkPhysicalDevice,
    ext_names: &HashSet<String>,
    get_video_caps: PfnGetVideoCapabilities,
    out: &mut HashMap<VideoFormat, Vec<u8>>,
) {
    unsafe {
        // H.264 encode: High profile
        if ext_names.contains(VK_KHR_VIDEO_ENCODE_H264_EXTENSION_NAME) {
            let profile_info = VkVideoEncodeH264ProfileInfoKHR {
                s_type: VK_STRUCTURE_TYPE_VIDEO_ENCODE_H264_PROFILE_INFO_KHR,
                p_next: ptr::null(),
                std_profile_idc: STD_VIDEO_H264_PROFILE_IDC_HIGH,
            };
            let depths = probe_bit_depths(
                device,
                VK_VIDEO_CODEC_OPERATION_ENCODE_H264_BIT_KHR,
                get_video_caps,
                &profile_info as *const _ as *const c_void,
                VK_STRUCTURE_TYPE_VIDEO_ENCODE_CAPABILITIES_KHR,
                VK_STRUCTURE_TYPE_VIDEO_ENCODE_H264_CAPABILITIES_KHR,
            );
            if !depths.is_empty() {
                out.insert(VideoFormat::H264, depths);
            }
        }

        // H.265 encode: Main / Main 10
        if ext_names.contains(VK_KHR_VIDEO_ENCODE_H265_EXTENSION_NAME) {
            let mut depths = Vec::new();

            let profile_8 = VkVideoEncodeH265ProfileInfoKHR {
                s_type: VK_STRUCTURE_TYPE_VIDEO_ENCODE_H265_PROFILE_INFO_KHR,
                p_next: ptr::null(),
                std_profile_idc: STD_VIDEO_H265_PROFILE_IDC_MAIN,
            };
            if try_profile(
                device,
                VK_VIDEO_CODEC_OPERATION_ENCODE_H265_BIT_KHR,
                VK_VIDEO_COMPONENT_BIT_DEPTH_8_BIT_KHR,
                &profile_8 as *const _ as *const c_void,
                VK_STRUCTURE_TYPE_VIDEO_ENCODE_CAPABILITIES_KHR,
                VK_STRUCTURE_TYPE_VIDEO_ENCODE_H265_CAPABILITIES_KHR,
                get_video_caps,
            ) {
                depths.push(8);
            }

            let profile_10 = VkVideoEncodeH265ProfileInfoKHR {
                s_type: VK_STRUCTURE_TYPE_VIDEO_ENCODE_H265_PROFILE_INFO_KHR,
                p_next: ptr::null(),
                std_profile_idc: STD_VIDEO_H265_PROFILE_IDC_MAIN_10,
            };
            if try_profile(
                device,
                VK_VIDEO_CODEC_OPERATION_ENCODE_H265_BIT_KHR,
                VK_VIDEO_COMPONENT_BIT_DEPTH_10_BIT_KHR,
                &profile_10 as *const _ as *const c_void,
                VK_STRUCTURE_TYPE_VIDEO_ENCODE_CAPABILITIES_KHR,
                VK_STRUCTURE_TYPE_VIDEO_ENCODE_H265_CAPABILITIES_KHR,
                get_video_caps,
            ) {
                depths.push(10);
            }

            if !depths.is_empty() {
                out.insert(VideoFormat::Hevc, depths);
            }
        }

        // AV1 encode: Main profile
        if ext_names.contains(VK_KHR_VIDEO_ENCODE_AV1_EXTENSION_NAME) {
            let profile_info = VkVideoEncodeAV1ProfileInfoKHR {
                s_type: VK_STRUCTURE_TYPE_VIDEO_ENCODE_AV1_PROFILE_INFO_KHR,
                p_next: ptr::null(),
                std_profile: STD_VIDEO_AV1_PROFILE_MAIN,
            };
            let depths = probe_bit_depths(
                device,
                VK_VIDEO_CODEC_OPERATION_ENCODE_AV1_BIT_KHR,
                get_video_caps,
                &profile_info as *const _ as *const c_void,
                VK_STRUCTURE_TYPE_VIDEO_ENCODE_CAPABILITIES_KHR,
                VK_STRUCTURE_TYPE_VIDEO_ENCODE_AV1_CAPABILITIES_KHR,
            );
            if !depths.is_empty() {
                out.insert(VideoFormat::Av1, depths);
            }
        }
    }
}

unsafe fn enumerate_device_extensions(
    vk: &VkLib,
    physical_device: VkPhysicalDevice,
) -> Result<HashSet<String>, FFPipelineError> {
    unsafe {
        let mut ext_count: u32 = 0;
        let result = (vk.vkEnumerateDeviceExtensionProperties)(
            physical_device,
            ptr::null(),
            &mut ext_count,
            ptr::null_mut(),
        );
        if result != VK_SUCCESS {
            return Err(FFPipelineError::VulkanCapabilitiesError(format!(
                "vkEnumerateDeviceExtensionProperties failed: {result}"
            )));
        }

        let mut extensions: Vec<VkExtensionProperties> =
            vec![std::mem::zeroed(); ext_count as usize];
        let result = (vk.vkEnumerateDeviceExtensionProperties)(
            physical_device,
            ptr::null(),
            &mut ext_count,
            extensions.as_mut_ptr(),
        );
        if result != VK_SUCCESS {
            return Err(FFPipelineError::VulkanCapabilitiesError(format!(
                "vkEnumerateDeviceExtensionProperties failed: {result}"
            )));
        }

        let names = extensions
            .iter()
            .map(|e| {
                CStr::from_ptr(e.extension_name.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();

        Ok(names)
    }
}

/// Build the required pNext chain for VkVideoCapabilitiesKHR and probe
/// a single codec_op + bit_depth combination.
///
/// The Vulkan spec requires:
///   VkVideoCapabilitiesKHR.pNext → decode/encode caps → codec-specific caps
unsafe fn try_profile(
    physical_device: VkPhysicalDevice,
    codec_op: u32,
    bit_depth_flag: u32,
    profile_p_next: *const c_void,
    decode_or_encode_caps_stype: u32,
    codec_caps_stype: u32,
    get_video_caps: PfnGetVideoCapabilities,
) -> bool {
    unsafe {
        let profile = VkVideoProfileInfoKHR {
            s_type: VK_STRUCTURE_TYPE_VIDEO_PROFILE_INFO_KHR,
            p_next: profile_p_next,
            video_codec_operation: codec_op,
            chroma_subsampling: VK_VIDEO_CHROMA_SUBSAMPLING_420_BIT_KHR,
            luma_bit_depth: bit_depth_flag,
            chroma_bit_depth: bit_depth_flag,
        };

        let mut codec_caps = CapsBuf::new(codec_caps_stype);
        let mut dec_enc_caps = CapsBuf::new(decode_or_encode_caps_stype);
        dec_enc_caps.p_next = &mut codec_caps as *mut _ as *mut c_void;

        let mut caps: VkVideoCapabilitiesKHR = std::mem::zeroed();
        caps.s_type = VK_STRUCTURE_TYPE_VIDEO_CAPABILITIES_KHR;
        caps.p_next = &mut dec_enc_caps as *mut _ as *mut c_void;

        let result = get_video_caps(physical_device, &profile, &mut caps);
        if result != VK_SUCCESS {
            log::debug!(
                "[vulkan]   codec_op=0x{:08x} bit_depth_flag=0x{:02x}: returned {}",
                codec_op,
                bit_depth_flag,
                result,
            );
        }
        result == VK_SUCCESS
    }
}

unsafe fn probe_bit_depths(
    physical_device: VkPhysicalDevice,
    codec_op: u32,
    get_video_caps: PfnGetVideoCapabilities,
    profile_p_next: *const c_void,
    decode_or_encode_caps_stype: u32,
    codec_caps_stype: u32,
) -> Vec<u8> {
    let mut bit_depths = Vec::new();
    for &(flag, value) in BIT_DEPTHS {
        if unsafe {
            try_profile(
                physical_device,
                codec_op,
                flag,
                profile_p_next,
                decode_or_encode_caps_stype,
                codec_caps_stype,
                get_video_caps,
            )
        } {
            bit_depths.push(value);
        }
    }
    bit_depths
}
