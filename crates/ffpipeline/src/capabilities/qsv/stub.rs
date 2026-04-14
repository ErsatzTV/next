use std::collections::HashSet;

use crate::capabilities::qsv::QsvCapabilities;
use crate::error::FFPipelineError;

impl QsvCapabilities {
    pub fn probe() -> Result<QsvCapabilities, FFPipelineError> {
        Ok(QsvCapabilities {
            supported_decoders: HashSet::new(),
            supported_encoders: HashSet::new(),
        })
    }
}
