use crate::ArgVec;
use crate::capabilities::videotoolbox::VideoToolboxCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel};
use crate::frame_size::FrameSize;
use crate::hw_accel::HwAccel;
use crate::pipeline::{FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;

#[derive(Debug, Clone)]
pub struct VideoToolbox {
    pub capabilities: VideoToolboxCapabilities,
}

impl VideoToolbox {
    pub fn new(capabilities: VideoToolboxCapabilities) -> Self {
        Self { capabilities }
    }
}

impl HwAccel for VideoToolbox {
    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        let format = match codec {
            "av1" => Some(VideoFormat::Av1),
            "h264" => Some(VideoFormat::H264),
            "hevc" => Some(VideoFormat::Hevc),
            "vp9" => Some(VideoFormat::Vp9),
            _ => None,
        };
        format.is_some_and(|f| self.capabilities.can_decode(&f, pixel_format.bit_depth()))
    }

    fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.capabilities.can_encode(format, bit_depth)
    }

    fn codec_for_format(
        &self,
        format: &VideoFormat,
        _video_size: Option<FrameSize>,
    ) -> Option<VideoCodec> {
        match format {
            VideoFormat::H264 if self.capabilities.can_encode(format, 8) => Some(VideoCodec {
                codec_name: "h264_videotoolbox",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::VideoToolbox,
            }),
            VideoFormat::Hevc if self.capabilities.can_encode(format, 8) => Some(VideoCodec {
                codec_name: "hevc_videotoolbox",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::VideoToolbox,
            }),
            _ => None,
        }
    }

    fn decoder_arg(&self) -> ArgVec {
        args![
            "-hwaccel",
            "videotoolbox",
            "-hwaccel_output_format",
            "videotoolbox_vld",
        ]
    }

    fn decoder_frame_surface(&self) -> FrameSurface {
        FrameSurface::VideoToolbox
    }

    fn initialize(&self, _ffmpeg_info: &FfmpegInfo, _is_hdr: bool) -> Self {
        self.clone()
    }

    fn init_hw_device(&self) -> ArgVec {
        Vec::new()
    }

    fn known_accel(&self) -> &KnownHardwareAccel {
        &KnownHardwareAccel::VideoToolbox
    }
}
