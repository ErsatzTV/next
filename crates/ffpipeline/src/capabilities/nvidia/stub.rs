use std::collections::HashMap;

use crate::capabilities::nvidia::NvidiaCapabilities;
use crate::error::FFPipelineError;

impl NvidiaCapabilities {
    pub(super) fn enable_background_teardown() {}

    pub(super) fn shutdown_probes() {}

    pub fn probe() -> Result<NvidiaCapabilities, FFPipelineError> {
        Ok(NvidiaCapabilities {
            supported_decoders: HashMap::new(),
            supported_encoders: HashMap::new(),
            device_uuid: None,
        })
    }

    pub(crate) fn probe_vulkan()
    -> Result<crate::capabilities::vulkan::VulkanCapabilities, FFPipelineError> {
        crate::capabilities::vulkan::VulkanCapabilities::probe_for_nvidia(None)
    }
}
