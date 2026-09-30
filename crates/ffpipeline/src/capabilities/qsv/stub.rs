use std::collections::{HashMap, HashSet};

use crate::capabilities::qsv::QsvCapabilities;
use crate::error::FFPipelineError;

impl QsvCapabilities {
    pub fn probe() -> Result<QsvCapabilities, FFPipelineError> {
        Ok(QsvCapabilities {
            supported_decoders: HashMap::new(),
            supported_encoders: HashMap::new(),
            upload_formats: HashSet::new(),
            convert_pairs: HashSet::new(),
            vpp_filters: HashSet::new(),
            rotation_formats: HashSet::new(),
            composite_pairs: HashSet::new(),
            runtime_api: None,
        })
    }
}
