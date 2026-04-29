use crate::capabilities::vaapi::VaapiCapabilities;
use crate::error::FFPipelineError;
use crate::error::FFPipelineError::VaapiCapabilitiesError;

impl VaapiCapabilities {
    pub fn probe(
        _device: &str,
        _driver: Option<&str>,
    ) -> Result<VaapiCapabilities, FFPipelineError> {
        Err(VaapiCapabilitiesError(String::from(
            "VAAPI is only supported on Linux",
        )))
    }
}
