use std::collections::HashMap;

use crate::capabilities::vulkan::VulkanCapabilities;
use crate::error::FFPipelineError;

impl VulkanCapabilities {
    pub fn probe() -> Result<VulkanCapabilities, FFPipelineError> {
        Ok(VulkanCapabilities {
            supported_decoders: HashMap::new(),
            supported_encoders: HashMap::new(),
        })
    }
}
