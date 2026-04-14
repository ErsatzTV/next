use crate::ffmpeg_info::FfmpegInfo;
use crate::filter_chain::PipelineFilter;
use crate::hw_accel::HwAccel;
use crate::pipeline::{FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::VideoFilter;

#[derive(Debug, Clone)]
pub struct VideoToolbox;

impl HwAccel for VideoToolbox {
    fn best_filter(&self, video_filter: &VideoFilter, _ffmpeg_info: &FfmpegInfo) -> VideoFilter {
        video_filter.clone()
    }

    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        match pixel_format.bit_depth() {
            10 => matches!(codec, "hevc"),
            8 => matches!(codec, "h264" | "hevc"),
            _ => false,
        }
    }

    fn codec_for_format(&self, format: &VideoFormat) -> VideoCodec {
        match format {
            VideoFormat::H264 => VideoCodec {
                codec_name: "h264_videotoolbox",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            },
            VideoFormat::Hevc => VideoCodec {
                codec_name: "hevc_videotoolbox",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            },
        }
    }

    fn decoder_arg(&self) -> Vec<String> {
        vec![
            String::from("-hwaccel"),
            String::from("videotoolbox"),
            String::from("-hwaccel_output_format"),
            String::from("videotoolbox_vld"),
        ]
    }

    fn decoder_filters(&self) -> Vec<PipelineFilter> {
        Vec::new()
    }

    fn envs(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    fn ffmpeg_name(&self) -> &str {
        "videotoolbox"
    }

    fn format_filter(&self, _pixel_format: &PixelFormat) -> Option<VideoFilter> {
        None
    }

    fn frame_surface(&self) -> FrameSurface {
        FrameSurface::VideoToolbox
    }

    fn init_hw_device(&self) -> Vec<String> {
        Vec::new()
    }

    fn output_format(&self, source_pixel_format: &PixelFormat) -> PixelFormat {
        match source_pixel_format.bit_depth() {
            10 => PixelFormat::P010le,
            _ => PixelFormat::Nv12,
        }
    }
}
