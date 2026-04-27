#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::c_void;

use libloading::Library;

pub enum _cl_platform_id {}
pub type cl_platform_id = *mut _cl_platform_id;

pub type cl_platform_info = u32;

pub enum _cl_device_id {}
pub type cl_device_id = *mut _cl_device_id;
pub type cl_device_type = u64;

pub const CL_SUCCESS: i32 = 0;
pub const CL_PLATFORM_NAME: cl_platform_info = 0x0902;
pub const CL_DEVICE_TYPE_ALL: cl_device_type = 0xFFFFFFFF;
pub const CL_DEVICE_TYPE_GPU: cl_device_type = 1 << 2;

pub struct ClLib {
    _lib: Library,
    pub clGetPlatformIDs: unsafe extern "C" fn(u32, *mut cl_platform_id, *mut u32) -> i32,
    pub clGetPlatformInfo: unsafe extern "C" fn(
        cl_platform_id,
        cl_platform_info,
        usize,
        *mut c_void,
        *mut usize,
    ) -> i32,
    pub clGetDeviceIDs: unsafe extern "C" fn(
        cl_platform_id,
        cl_device_type,
        u32,
        *mut cl_device_id,
        *mut u32,
    ) -> i32,
}

impl ClLib {
    pub fn load() -> Result<Self, libloading::Error> {
        #[cfg(target_os = "linux")]
        let name = "libOpenCL.so";
        #[cfg(target_os = "windows")]
        let name = "OpenCL.dll";
        unsafe {
            let lib = Library::new(name)?;
            let clGetPlatformIDs = *lib.get(b"clGetPlatformIDs\0")?;
            let clGetPlatformInfo = *lib.get(b"clGetPlatformInfo\0")?;
            let clGetDeviceIDs = *lib.get(b"clGetDeviceIDs\0")?;
            Ok(Self {
                _lib: lib,
                clGetPlatformIDs,
                clGetPlatformInfo,
                clGetDeviceIDs,
            })
        }
    }
}

#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
#[cfg(test)]
mod tests {
    use std::ffi::{CStr, c_char};

    use super::*;

    #[test]
    #[ignore]
    fn get_platforms() {
        let cl_load = ClLib::load();
        match cl_load {
            Ok(cl) => {
                let mut platform_count: u32 = 0;
                unsafe {
                    let result =
                        (cl.clGetPlatformIDs)(0, std::ptr::null_mut(), &mut platform_count);
                    assert_eq!(result, CL_SUCCESS);
                };

                println!("platform count: {platform_count}");

                if platform_count > 0 {
                    let mut platforms: Vec<cl_platform_id> =
                        vec![std::ptr::null_mut(); platform_count as usize];
                    unsafe {
                        let result = (cl.clGetPlatformIDs)(
                            platform_count,
                            platforms.as_mut_ptr(),
                            std::ptr::null_mut(),
                        );
                        assert_eq!(result, CL_SUCCESS);
                    }

                    for platform in platforms {
                        let mut info_size: usize = 0;
                        unsafe {
                            let result = (cl.clGetPlatformInfo)(
                                platform,
                                CL_PLATFORM_NAME,
                                0,
                                std::ptr::null_mut(),
                                &mut info_size,
                            );
                            assert_eq!(result, CL_SUCCESS);
                        }

                        println!("info size: {info_size}");

                        let mut info = vec![0 as c_char; info_size];
                        unsafe {
                            let result = (cl.clGetPlatformInfo)(
                                platform,
                                CL_PLATFORM_NAME,
                                info_size,
                                info.as_mut_ptr() as *mut c_void,
                                std::ptr::null_mut(),
                            );
                            assert_eq!(result, CL_SUCCESS);
                        }

                        let value = unsafe {
                            CStr::from_ptr(info.as_ptr() as *const c_char)
                                .to_string_lossy()
                                .into_owned()
                        };

                        println!("{value}");

                        let mut count: u32 = 0;
                        unsafe {
                            let result = (cl.clGetDeviceIDs)(
                                platform,
                                CL_DEVICE_TYPE_GPU,
                                0,
                                std::ptr::null_mut(),
                                &mut count,
                            );
                            assert_eq!(result, CL_SUCCESS);
                        }

                        println!("gpu device count: {count}");
                    }
                }
            }
            Err(err) => {
                println!("failed to load OpenCL: {err}");
            }
        }
    }
}
