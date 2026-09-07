#![allow(non_snake_case)]

use std::ffi::{c_char, c_void};
use std::mem::offset_of;

use libloading::Library;

use crate::{
    VkExtensionProperties, VkInstance, VkInstanceCreateInfo, VkPhysicalDevice,
    VkPhysicalDeviceIDProperties, VkPhysicalDeviceProperties, VkPhysicalDeviceProperties2,
    VkResult,
};

// VkPhysicalDeviceLimits contains VkDeviceSize members, so it is 8-byte aligned and
// pipeline_cache_uuid is padded out to offset 296. Getting this wrong makes
// vkGetPhysicalDeviceProperties write past the end of the struct.
const _: () = assert!(size_of::<VkPhysicalDeviceProperties>() == 824);
const _: () = assert!(offset_of!(VkPhysicalDeviceProperties, limits) == 296);
const _: () = assert!(offset_of!(VkPhysicalDeviceProperties, sparse_properties) == 800);
const _: () = assert!(size_of::<VkPhysicalDeviceProperties2>() == 840);
const _: () = assert!(offset_of!(VkPhysicalDeviceProperties2, properties) == 16);
const _: () = assert!(size_of::<VkPhysicalDeviceIDProperties>() == 64);
const _: () = assert!(offset_of!(VkPhysicalDeviceIDProperties, device_uuid) == 16);

pub type PfnVkVoidFunction = unsafe extern "C" fn();

pub struct VkLib {
    _lib: Library,
    pub vkCreateInstance: unsafe extern "C" fn(
        *const VkInstanceCreateInfo,
        *const c_void,
        *mut VkInstance,
    ) -> VkResult,
    pub vkDestroyInstance: unsafe extern "C" fn(VkInstance, *const c_void),
    pub vkEnumeratePhysicalDevices:
        unsafe extern "C" fn(VkInstance, *mut u32, *mut VkPhysicalDevice) -> VkResult,
    pub vkEnumerateDeviceExtensionProperties: unsafe extern "C" fn(
        VkPhysicalDevice,
        *const c_char,
        *mut u32,
        *mut VkExtensionProperties,
    ) -> VkResult,
    pub vkGetPhysicalDeviceProperties:
        unsafe extern "C" fn(VkPhysicalDevice, *mut VkPhysicalDeviceProperties),
    pub vkGetPhysicalDeviceProperties2:
        unsafe extern "C" fn(VkPhysicalDevice, *mut VkPhysicalDeviceProperties2),
    pub vkGetInstanceProcAddr:
        unsafe extern "C" fn(VkInstance, *const c_char) -> Option<PfnVkVoidFunction>,
}

impl VkLib {
    pub fn load() -> Result<Self, libloading::Error> {
        unsafe {
            #[cfg(target_os = "linux")]
            let lib = Library::new("libvulkan.so.1")?;

            #[cfg(target_os = "windows")]
            let lib = Library::new("vulkan-1.dll")?;

            let vkCreateInstance = *lib.get(b"vkCreateInstance\0")?;
            let vkDestroyInstance = *lib.get(b"vkDestroyInstance\0")?;
            let vkEnumeratePhysicalDevices = *lib.get(b"vkEnumeratePhysicalDevices\0")?;
            let vkEnumerateDeviceExtensionProperties =
                *lib.get(b"vkEnumerateDeviceExtensionProperties\0")?;
            let vkGetPhysicalDeviceProperties = *lib.get(b"vkGetPhysicalDeviceProperties\0")?;
            let vkGetPhysicalDeviceProperties2 = *lib.get(b"vkGetPhysicalDeviceProperties2\0")?;
            let vkGetInstanceProcAddr = *lib.get(b"vkGetInstanceProcAddr\0")?;

            Ok(Self {
                _lib: lib,
                vkCreateInstance,
                vkDestroyInstance,
                vkEnumeratePhysicalDevices,
                vkEnumerateDeviceExtensionProperties,
                vkGetPhysicalDeviceProperties,
                vkGetPhysicalDeviceProperties2,
                vkGetInstanceProcAddr,
            })
        }
    }
}
