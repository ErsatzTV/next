use crate::ArgVec;

#[derive(Copy, Clone, PartialEq)]
pub enum AudioCodec {
    Copy,
    Aac,
    Ac3,
}

impl AudioCodec {
    pub(crate) fn as_arg(&self) -> ArgVec {
        let codec = match self {
            AudioCodec::Copy => "copy",
            AudioCodec::Aac => "aac",
            AudioCodec::Ac3 => "ac3",
        };

        args!["-acodec", codec]
    }
}
