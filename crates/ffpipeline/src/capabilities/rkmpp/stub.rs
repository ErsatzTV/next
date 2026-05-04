use std::collections::HashSet;

use crate::capabilities::rkmpp::RkmppCapabilities;
use crate::error::FFPipelineError;

impl RkmppCapabilities {
    pub fn probe() -> Result<RkmppCapabilities, FFPipelineError> {
        Ok(RkmppCapabilities {
            supported_decoders: HashSet::new(),
            supported_encoders: HashSet::new(),
        })
    }
}
