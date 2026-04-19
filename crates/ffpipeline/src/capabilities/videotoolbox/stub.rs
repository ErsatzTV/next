use std::collections::HashSet;

use crate::capabilities::videotoolbox::VideoToolboxCapabilities;
use crate::error::FFPipelineError;

impl VideoToolboxCapabilities {
    pub fn probe() -> Result<VideoToolboxCapabilities, FFPipelineError> {
        Ok(VideoToolboxCapabilities {
            supported_decoders: HashSet::new(),
            supported_encoders: HashSet::new(),
        })
    }
}
