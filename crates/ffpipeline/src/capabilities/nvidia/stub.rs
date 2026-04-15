use std::collections::HashSet;

use crate::capabilities::nvidia::NvidiaCapabilities;
use crate::error::FFPipelineError;

impl NvidiaCapabilities {
    pub fn probe() -> Result<NvidiaCapabilities, FFPipelineError> {
        Ok(NvidiaCapabilities {
            supported_decoders: HashSet::new(),
            supported_encoders: HashSet::new(),
        })
    }
}
