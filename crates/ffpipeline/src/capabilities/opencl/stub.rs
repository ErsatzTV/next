use crate::capabilities::opencl::OpenCLCapabilities;
use crate::error::FFPipelineError;

impl OpenCLCapabilities {
    pub fn probe() -> Result<OpenCLCapabilities, FFPipelineError> {
        Ok(OpenCLCapabilities {
            platform_count: 0,
            gpu_device_count: 0,
        })
    }
}
