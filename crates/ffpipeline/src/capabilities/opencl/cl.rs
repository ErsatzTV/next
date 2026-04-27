use libcl_sys::{CL_DEVICE_TYPE_GPU, CL_SUCCESS, ClLib, cl_platform_id};

use crate::capabilities::opencl::OpenCLCapabilities;
use crate::error::FFPipelineError;

impl OpenCLCapabilities {
    pub fn probe() -> Result<OpenCLCapabilities, FFPipelineError> {
        let cl = ClLib::load().map_err(|e| {
            FFPipelineError::OpenCLCapabilitiesError(format!("libOpenCL not found: {e}"))
        })?;

        let (platform_count, gpu_device_count) = get_platform_and_gpu_device_count(cl);

        Ok(OpenCLCapabilities {
            platform_count,
            gpu_device_count,
        })
    }
}

fn get_platform_and_gpu_device_count(cl: ClLib) -> (u32, u32) {
    let mut platform_count: u32 = 0;
    let mut gpu_device_count: u32 = 0;
    let mut result: i32;

    // get platform count
    unsafe {
        result = (cl.clGetPlatformIDs)(0, std::ptr::null_mut(), &mut platform_count);
    };

    if result == CL_SUCCESS && platform_count > 0 {
        let mut platforms: Vec<cl_platform_id> =
            vec![std::ptr::null_mut(); platform_count as usize];
        // get platforms
        unsafe {
            result =
                (cl.clGetPlatformIDs)(platform_count, platforms.as_mut_ptr(), std::ptr::null_mut());
        }
        if result == CL_SUCCESS {
            for platform in platforms {
                // get device count
                unsafe {
                    let mut count: u32 = 0;
                    result = (cl.clGetDeviceIDs)(
                        platform,
                        CL_DEVICE_TYPE_GPU,
                        0,
                        std::ptr::null_mut(),
                        &mut count,
                    );
                    if result == CL_SUCCESS {
                        gpu_device_count += count;
                    }
                }
            }
        }
    }

    (platform_count, gpu_device_count)
}
