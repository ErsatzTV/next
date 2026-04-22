use std::collections::{HashMap, HashSet};

use crate::capabilities::qsv::QsvCapabilities;
use crate::error::FFPipelineError;

impl QsvCapabilities {
    pub fn probe() -> Result<QsvCapabilities, FFPipelineError> {
        Ok(QsvCapabilities {
            supported_decoders: HashMap::new(),
            supported_encoders: HashMap::new(),
            vpp_pixel_formats: HashSet::new(),
        })
    }
}
