use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel};
use crate::hw_accel::HwAccel;
use crate::pipeline::{FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;

#[derive(Debug, Clone)]
pub struct VideoToolbox;

impl HwAccel for VideoToolbox {
    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        match pixel_format.bit_depth() {
            10 => matches!(codec, "hevc"),
            8 => matches!(codec, "h264" | "hevc"),
            _ => false,
        }
    }

    fn codec_for_format(&self, format: &VideoFormat) -> Option<VideoCodec> {
        match format {
            VideoFormat::H264 => Some(VideoCodec {
                codec_name: "h264_videotoolbox",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            }),
            VideoFormat::Hevc => Some(VideoCodec {
                codec_name: "hevc_videotoolbox",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            }),
            _ => None,
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

    fn decoder_frame_surface(&self) -> FrameSurface {
        FrameSurface::VideoToolbox
    }

    fn encoder_frame_surface(&self) -> FrameSurface {
        FrameSurface::VideoToolbox
    }

    fn initialize(&self, _ffmpeg_info: &FfmpegInfo, _is_hdr: bool) -> Self {
        self.clone()
    }

    fn init_hw_device(&self) -> Vec<String> {
        Vec::new()
    }

    fn known_accel(&self) -> &KnownHardwareAccel {
        &KnownHardwareAccel::VideoToolbox
    }
}
