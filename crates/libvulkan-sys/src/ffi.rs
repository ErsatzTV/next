#![allow(non_snake_case)]

use std::ffi::{c_char, c_void};

use libloading::Library;

use crate::{
    VkExtensionProperties, VkInstance, VkInstanceCreateInfo, VkPhysicalDevice,
    VkPhysicalDeviceProperties, VkResult,
};

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
            let vkGetInstanceProcAddr = *lib.get(b"vkGetInstanceProcAddr\0")?;

            Ok(Self {
                _lib: lib,
                vkCreateInstance,
                vkDestroyInstance,
                vkEnumeratePhysicalDevices,
                vkEnumerateDeviceExtensionProperties,
                vkGetPhysicalDeviceProperties,
                vkGetInstanceProcAddr,
            })
        }
    }
}
