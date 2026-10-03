use crate::capabilities::videotoolbox::VideoToolboxCapabilities;
use crate::error::FFPipelineError;
use crate::frame_size::FrameSize;
use crate::pipeline::VideoFormat;

impl VideoToolboxCapabilities {
    pub fn probe() -> Result<VideoToolboxCapabilities, FFPipelineError> {
        Ok(VideoToolboxCapabilities::default())
    }

    pub(crate) fn probe_encode_size(_format: &VideoFormat, _size: FrameSize) -> bool {
        false
    }
}
