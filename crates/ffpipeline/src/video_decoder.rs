use crate::ArgVec;
use crate::ffmpeg_info::FfmpegInfo;
use crate::filter_chain::PipelineFilter;
use crate::hw_accel::{HardwareAccel, HwAccel, HwDecoder};
use crate::output_settings::OutputSettings;
use crate::pipeline::{FrameSurface, PixelFormat};
use crate::probe::ProbeResultVideoStream;

pub enum VideoDecoder {
    None,
    Software,
    HardwareAccel {
        accel: Box<HardwareAccel>,
        decoder: HwDecoder,
    },
}

impl VideoDecoder {
    pub(crate) fn new(
        ffmpeg_info: &FfmpegInfo,
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
                if let Some(decoder) = accel.make_decoder(ffmpeg_info, video_stream) {
                    VideoDecoder::HardwareAccel {
                        accel: Box::new(accel.clone()),
                        decoder,
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
            VideoDecoder::HardwareAccel { decoder, .. } => decoder.filters.clone(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn output_surface(&self) -> FrameSurface {
        match self {
            VideoDecoder::None => FrameSurface::System,
            VideoDecoder::Software => FrameSurface::System,
            VideoDecoder::HardwareAccel { decoder, .. } => decoder.surface,
        }
    }

    pub(crate) fn output_format(&self, source_pixel_format: &PixelFormat) -> PixelFormat {
        match self {
            VideoDecoder::None => *source_pixel_format,
            VideoDecoder::Software => *source_pixel_format,
            VideoDecoder::HardwareAccel { accel, .. } => {
                accel.output_format(source_pixel_format).into()
            }
        }
    }

    pub(crate) fn as_arg(&self) -> ArgVec {
        match self {
            VideoDecoder::None => Vec::new(),
            VideoDecoder::Software => Vec::new(),
            VideoDecoder::HardwareAccel { decoder, .. } => decoder.args.clone(),
        }
    }
}
