use std::ffi::{CStr, c_char, c_void};

use libcl_sys::{
    CL_DEVICE_EXTENSIONS, CL_DEVICE_TYPE_GPU, CL_PLATFORM_EXTENSIONS, CL_SUCCESS, ClLib,
    cl_device_id, cl_platform_id,
};

use crate::capabilities::opencl::OpenCLCapabilities;
use crate::error::FFPipelineError;

/// ffmpeg's only VAAPI to OpenCL map; it checks platform and device
const VA_API_MEDIA_SHARING: &str = "cl_intel_va_api_media_sharing";

impl OpenCLCapabilities {
    pub fn probe() -> Result<OpenCLCapabilities, FFPipelineError> {
        let cl = ClLib::load().map_err(|e| {
            FFPipelineError::OpenCLCapabilitiesError(format!("libOpenCL not found: {e}"))
        })?;

        Ok(OpenCLCapabilities {
            va_api_sharing_device_count: count_va_api_sharing_devices(&cl),
        })
    }
}

fn count_va_api_sharing_devices(cl: &ClLib) -> u32 {
    platforms(cl)
        .into_iter()
        .filter(|&p| has_extension(&platform_extensions(cl, p)))
        .flat_map(|p| gpu_devices(cl, p))
        .filter(|&d| has_extension(&device_extensions(cl, d)))
        .count() as u32
}

fn has_extension(extensions: &str) -> bool {
    extensions
        .split_whitespace()
        .any(|e| e == VA_API_MEDIA_SHARING)
}

fn platforms(cl: &ClLib) -> Vec<cl_platform_id> {
    let mut count: u32 = 0;
    if unsafe { (cl.clGetPlatformIDs)(0, std::ptr::null_mut(), &mut count) } != CL_SUCCESS
        || count == 0
    {
        return Vec::new();
    }

    let mut platforms = vec![std::ptr::null_mut(); count as usize];
    if unsafe { (cl.clGetPlatformIDs)(count, platforms.as_mut_ptr(), std::ptr::null_mut()) }
        != CL_SUCCESS
    {
        return Vec::new();
    }
    platforms
}

fn gpu_devices(cl: &ClLib, platform: cl_platform_id) -> Vec<cl_device_id> {
    let mut count: u32 = 0;
    if unsafe {
        (cl.clGetDeviceIDs)(
            platform,
            CL_DEVICE_TYPE_GPU,
            0,
            std::ptr::null_mut(),
            &mut count,
        )
    } != CL_SUCCESS
        || count == 0
    {
        return Vec::new();
    }

    let mut devices = vec![std::ptr::null_mut(); count as usize];
    if unsafe {
        (cl.clGetDeviceIDs)(
            platform,
            CL_DEVICE_TYPE_GPU,
            count,
            devices.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    } != CL_SUCCESS
    {
        return Vec::new();
    }
    devices
}

fn platform_extensions(cl: &ClLib, platform: cl_platform_id) -> String {
    info_string(|size, value, size_ret| unsafe {
        (cl.clGetPlatformInfo)(platform, CL_PLATFORM_EXTENSIONS, size, value, size_ret)
    })
}

fn device_extensions(cl: &ClLib, device: cl_device_id) -> String {
    info_string(|size, value, size_ret| unsafe {
        (cl.clGetDeviceInfo)(device, CL_DEVICE_EXTENSIONS, size, value, size_ret)
    })
}

fn info_string(get: impl Fn(usize, *mut c_void, *mut usize) -> i32) -> String {
    let mut size: usize = 0;
    if get(0, std::ptr::null_mut(), &mut size) != CL_SUCCESS || size == 0 {
        return String::new();
    }

    let mut value = vec![0 as c_char; size];
    if get(
        size,
        value.as_mut_ptr() as *mut c_void,
        std::ptr::null_mut(),
    ) != CL_SUCCESS
    {
        return String::new();
    }
    // in case the driver omits the nul
    value[size - 1] = 0;
    unsafe { CStr::from_ptr(value.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_extension_matches_whole_names_only() {
        assert!(has_extension(
            "cl_khr_fp16 cl_intel_va_api_media_sharing cl_khr_icd "
        ));
        assert!(!has_extension(
            "cl_khr_fp16 cl_intel_va_api_media_sharing_foo"
        ));
        assert!(!has_extension(""));
    }
}
