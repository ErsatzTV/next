use std::collections::HashMap;

use crate::capabilities::vulkan::VulkanCapabilities;
use crate::error::FFPipelineError;

impl VulkanCapabilities {
    pub fn probe() -> Result<VulkanCapabilities, FFPipelineError> {
        Ok(VulkanCapabilities {
            device_index: 0,
            supported_decoders: HashMap::new(),
            supported_encoders: HashMap::new(),
        })
    }
}
