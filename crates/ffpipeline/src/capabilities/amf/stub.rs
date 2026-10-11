use std::collections::{HashMap, HashSet};

use crate::capabilities::amf::{AmfCapabilities, AmfDeviceTarget};
use crate::error::FFPipelineError;

impl AmfCapabilities {
    pub fn probe_with(_target: AmfDeviceTarget) -> Result<AmfCapabilities, FFPipelineError> {
        Ok(AmfCapabilities {
            supported_decoders: HashMap::new(),
            supported_encoders: HashMap::new(),
            vpp_input_formats: HashSet::new(),
            vpp_output_formats: HashSet::new(),
            runtime_version: None,
            device: None,
            adapter: None,
        })
    }
}
