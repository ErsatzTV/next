use crate::ArgVec;
use crate::filter_chain::PipelineFilter;
use crate::hw_accel::{HardwareAccel, HwAccel};
use crate::output_settings::OutputSettings;
use crate::pipeline::{FrameSurface, PixelFormat};
use crate::probe::ProbeResultVideoStream;

pub enum VideoDecoder {
    None,
    Software,
    HardwareAccel { accel: HardwareAccel },
}

impl VideoDecoder {
    pub(crate) fn new(
        video_stream: &ProbeResultVideoStream,
        is_still_image: bool,
        output_settings: &OutputSettings,
    ) -> VideoDecoder {
        // stream copy should not have a decoder; still image should not use accel
        if output_settings.video_format.is_none() || is_still_image {
            return VideoDecoder::None;
        }

        match &output_settings.accel {
            Some(accel) => {
                if Self::can_hw_decode(
                    accel,
                    &video_stream.codec,
                    &video_stream.profile,
                    &video_stream.pix_fmt,
                ) {
                    VideoDecoder::HardwareAccel {
                        accel: accel.clone(),
                    }
                } else {
                    VideoDecoder::Software
                }
            }
            None => VideoDecoder::Software,
        }
    }

    pub(crate) fn filters(&self) -> Vec<PipelineFilter> {
        match self {
            VideoDecoder::HardwareAccel { accel } => accel.decoder_filters(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn output_surface(&self) -> FrameSurface {
        match self {
            VideoDecoder::None => FrameSurface::System,
            VideoDecoder::Software => FrameSurface::System,
            VideoDecoder::HardwareAccel { accel } => accel.decoder_frame_surface(),
        }
    }

    pub(crate) fn output_format(&self, source_pixel_format: &PixelFormat) -> PixelFormat {
        match self {
            VideoDecoder::None => source_pixel_format.clone(),
            VideoDecoder::Software => source_pixel_format.clone(),
            VideoDecoder::HardwareAccel { accel } => accel.output_format(source_pixel_format),
        }
    }

    pub(crate) fn as_arg(&self) -> ArgVec {
        match self {
            VideoDecoder::None => Vec::new(),
            VideoDecoder::Software => Vec::new(),
            VideoDecoder::HardwareAccel { accel } => accel.decoder_arg(),
        }
    }

    fn can_hw_decode(accel: &HardwareAccel, codec: &str, profile: &str, pix_fmt: &str) -> bool {
        accel.can_decode(codec, profile, &PixelFormat::parse(pix_fmt))
    }
}
