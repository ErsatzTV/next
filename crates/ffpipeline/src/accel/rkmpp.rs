use crate::ArgVec;
use crate::capabilities::rkmpp::RkmppCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel};
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::pipeline::{FrameSurface, PixelFormat, SurfaceSet, VideoFormat};
use crate::probe::ProbeResultVideoStream;
use crate::video_codec::VideoCodec;

#[derive(Debug, Clone)]
pub struct Rkmpp {
    pub capabilities: RkmppCapabilities,
}

impl HwAccel for Rkmpp {
    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        let format = match codec {
            "h264" => Some(VideoFormat::H264),
            "hevc" => Some(VideoFormat::Hevc),
            "vp8" => Some(VideoFormat::Vp8),
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
        _bit_depth: u8,
        _video_size: Option<FrameSize>,
    ) -> Option<VideoCodec> {
        match format {
            VideoFormat::H264 if self.capabilities.can_encode(format, 8) => Some(VideoCodec {
                codec_name: "h264_rkmpp",
                options: Vec::new(),
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: None,
                preferred_surface: FrameSurface::Rkmpp,
            }),
            VideoFormat::Hevc if self.capabilities.can_encode(format, 8) => Some(VideoCodec {
                codec_name: "hevc_rkmpp",
                options: Vec::new(),
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: None,
                preferred_surface: FrameSurface::Rkmpp,
            }),
            _ => None,
        }
    }

    fn init_hw_device(&self, _surfaces: &SurfaceSet) -> ArgVec {
        Vec::new()
    }

    fn known_accel(&self) -> Option<&KnownHardwareAccel> {
        Some(&KnownHardwareAccel::Rkmpp)
    }

    fn make_decoder(
        &self,
        _ffmpeg_info: &FfmpegInfo,
        video_stream: &ProbeResultVideoStream,
    ) -> Option<HwDecoder> {
        if self.can_decode(
            &video_stream.codec,
            &video_stream.profile,
            &PixelFormat::parse(&video_stream.pix_fmt),
        ) {
            Some(HwDecoder {
                args: args!["-hwaccel", "rkmpp", "-hwaccel_output_format", "drm_prime",],
                surface: FrameSurface::Rkmpp,
                filters: Vec::new(),
            })
        } else {
            None
        }
    }
}
