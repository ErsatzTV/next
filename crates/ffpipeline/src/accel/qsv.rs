use crate::ffmpeg_info::{FfmpegInfo, KnownVideoFilter};
use crate::filter_chain::PipelineFilter;
use crate::frame_size::FrameSize;
use crate::hw_accel::HwAccel;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::{HwVideoFilter, VideoFilter};

#[derive(Debug, Clone)]
pub struct Qsv;

impl HwAccel for Qsv {
    fn best_filter(&self, video_filter: &VideoFilter, ffmpeg_info: &FfmpegInfo) -> VideoFilter {
        match video_filter {
            VideoFilter::Scale { size, .. }
                if ffmpeg_info.has_video_filter(&KnownVideoFilter::VppQsv) =>
            {
                VideoFilter::Hardware(Box::new(ScaleQsv {
                    size: size.clone(),
                    //input_is_anamorphic: *input_is_anamorphic,
                    //force_original_aspect_ratio: force_original_aspect_ratio.clone(),
                }))
            }
            _ => video_filter.clone(),
        }
    }

    fn codec_for_format(&self, format: &VideoFormat) -> VideoCodec {
        match format {
            VideoFormat::H264 => VideoCodec {
                codec_name: "h264_qsv",
                options: &["-low_power", "0", "-look_ahead", "0"],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            },
            VideoFormat::Hevc => VideoCodec {
                codec_name: "hevc_qsv",
                options: &["-low_power", "0", "-look_ahead", "0"],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            },
        }
    }

    fn decoder_arg(&self) -> Vec<String> {
        vec![
            String::from("-hwaccel"),
            String::from("qsv"),
            String::from("-hwaccel_output_format"),
            String::from("qsv"),
        ]
    }

    fn decoder_filters(&self) -> Vec<PipelineFilter> {
        Vec::new()
    }

    fn envs(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    fn ffmpeg_name(&self) -> &str {
        "qsv"
    }

    fn format_filter(&self, _pixel_format: &PixelFormat) -> Option<VideoFilter> {
        None
    }

    fn frame_surface(&self) -> FrameSurface {
        FrameSurface::Qsv
    }

    fn init_hw_device(&self) -> Vec<String> {
        vec![
            String::from("-init_hw_device"),
            String::from("qsv=hw"),
            String::from("-filter_hw_device"),
            String::from("hw"),
        ]
    }

    fn output_format(&self, source_pixel_format: &PixelFormat) -> PixelFormat {
        match source_pixel_format.bit_depth() {
            10 => PixelFormat::P010le,
            _ => PixelFormat::Nv12,
        }
    }
}

#[derive(Clone)]
struct ScaleQsv {
    size: Option<FrameSize>,
}

impl HwVideoFilter for ScaleQsv {
    fn evaluate(&self, _state: &FrameState) -> Option<VideoFilter> {
        // called before this is used
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = size.clone();
            state.surface = FrameSurface::Qsv;
            // TODO: anamorphic handling
        }
    }

    fn required_surface(&self) -> FrameSurface {
        FrameSurface::Qsv
    }

    fn as_arg(&self) -> Option<String> {
        self.size.as_ref().map(|s|
            // TODO: anamorphic handling
            format!("vpp_qsv=w={}:h={}", s.width, s.height))
    }
}
