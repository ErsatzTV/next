use std::collections::{HashMap, HashSet};

use crate::capabilities::qsv::QsvCapabilities;
use crate::error::FFPipelineError;

impl QsvCapabilities {
    pub fn probe() -> Result<QsvCapabilities, FFPipelineError> {
        Ok(QsvCapabilities {
            supported_decoders: HashMap::new(),
            supported_encoders: HashMap::new(),
            vpp_pixel_formats: HashSet::new(),
            vpp_filters: HashSet::new(),
            rotation_formats: HashSet::new(),
            composite_formats: HashSet::new(),
            runtime_api: None,
        })
    }

    pub fn diagnostics() -> Result<String, FFPipelineError> {
        Err(FFPipelineError::QsvCapabilitiesError(
            "QSV is not supported on this platform".into(),
        ))
    }
}
