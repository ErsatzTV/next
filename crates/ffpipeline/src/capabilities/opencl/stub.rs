use crate::capabilities::opencl::OpenCLCapabilities;
use crate::error::FFPipelineError;

impl OpenCLCapabilities {
    pub fn probe() -> Result<OpenCLCapabilities, FFPipelineError> {
        Ok(OpenCLCapabilities {
            va_api_sharing_device_count: 0,
        })
    }
}
