use std::collections::HashMap;

use crate::capabilities::nvidia::NvidiaCapabilities;
use crate::error::FFPipelineError;

impl NvidiaCapabilities {
    pub fn probe() -> Result<NvidiaCapabilities, FFPipelineError> {
        Ok(NvidiaCapabilities {
            supported_decoders: HashMap::new(),
            supported_encoders: HashMap::new(),
        })
    }
}
