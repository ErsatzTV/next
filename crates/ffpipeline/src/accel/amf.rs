use crate::ArgVec;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel};
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::pipeline::{FrameSurface, PixelFormat, SurfaceSet, VideoFormat};
use crate::probe::ProbeResultVideoStream;
use crate::video_codec::VideoCodec;

#[derive(Debug, Clone)]
pub struct Amf;

impl HwAccel for Amf {
    fn codec_for_format(
        &self,
        format: &VideoFormat,
        _video_size: Option<FrameSize>,
    ) -> Option<VideoCodec> {
        match format {
            VideoFormat::H264 => Some(VideoCodec {
                codec_name: "h264_amf",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::Amf,
            }),
            VideoFormat::Hevc => Some(VideoCodec {
                codec_name: "hevc_amf",
                options: &["-tag:v", "hvc1"],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::Amf,
            }),
            _ => None,
        }
    }

    fn init_hw_device(&self, _surfaces: &SurfaceSet) -> ArgVec {
        Vec::new()
    }

    fn known_accel(&self) -> Option<&KnownHardwareAccel> {
        None
    }

    fn make_decoder(
        &self,
        _ffmpeg_info: &FfmpegInfo,
        _video_stream: &ProbeResultVideoStream,
    ) -> Option<HwDecoder> {
        None
    }
}
