use crate::accel;
use crate::ffmpeg_info::FfmpegInfo;
use crate::filter_chain::PipelineFilter;
use crate::pipeline::{FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::VideoFilter;

pub trait HwAccel {
    fn best_filter(&self, video_filter: &VideoFilter, ffmpeg_info: &FfmpegInfo) -> VideoFilter;
    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        match pixel_format.bit_depth() {
            10 => matches!(codec, "av1" | "hevc"),
            8 => matches!(codec, "av1" | "h264" | "hevc" | "mpeg2video"),
            _ => false,
        }
    }
    fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        match bit_depth {
            10 => matches!(format, VideoFormat::Hevc),
            8 => matches!(format, VideoFormat::H264 | VideoFormat::Hevc),
            _ => false,
        }
    }
    fn codec_for_format(&self, format: &VideoFormat) -> VideoCodec;
    fn decoder_arg(&self) -> Vec<String>;
    fn decoder_filters(&self) -> Vec<PipelineFilter>;
    fn envs(&self) -> Vec<(String, String)>;
    fn ffmpeg_name(&self) -> &str;
    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter>;
    fn frame_surface(&self) -> FrameSurface;
    fn init_hw_device(&self) -> Vec<String>;
    fn output_format(&self, source_pixel_format: &PixelFormat) -> PixelFormat;
    fn supports_pixel_format(&self, _pixel_format: &PixelFormat) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
pub enum HardwareAccel {
    Cuda(accel::cuda::Cuda),
    Qsv(accel::qsv::Qsv),
    Vaapi(accel::vaapi::Vaapi),
    VideoToolbox(accel::video_toolbox::VideoToolbox),
}

impl HwAccel for HardwareAccel {
    fn best_filter(&self, video_filter: &VideoFilter, ffmpeg_info: &FfmpegInfo) -> VideoFilter {
        match self {
            Self::Cuda(a) => a.best_filter(video_filter, ffmpeg_info),
            Self::Qsv(a) => a.best_filter(video_filter, ffmpeg_info),
            Self::Vaapi(a) => a.best_filter(video_filter, ffmpeg_info),
            Self::VideoToolbox(a) => a.best_filter(video_filter, ffmpeg_info),
        }
    }

    fn can_decode(&self, codec: &str, profile: &str, pixel_format: &PixelFormat) -> bool {
        match self {
            Self::Cuda(a) => a.can_decode(codec, profile, pixel_format),
            Self::Qsv(a) => a.can_decode(codec, profile, pixel_format),
            Self::Vaapi(a) => a.can_decode(codec, profile, pixel_format),
            Self::VideoToolbox(a) => a.can_decode(codec, profile, pixel_format),
        }
    }

    fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        match self {
            Self::Cuda(a) => a.can_encode(format, bit_depth),
            Self::Qsv(a) => a.can_encode(format, bit_depth),
            Self::Vaapi(a) => a.can_encode(format, bit_depth),
            Self::VideoToolbox(a) => a.can_encode(format, bit_depth),
        }
    }

    fn codec_for_format(&self, format: &VideoFormat) -> VideoCodec {
        match self {
            Self::Cuda(a) => a.codec_for_format(format),
            Self::Qsv(a) => a.codec_for_format(format),
            Self::Vaapi(a) => a.codec_for_format(format),
            Self::VideoToolbox(a) => a.codec_for_format(format),
        }
    }

    fn decoder_arg(&self) -> Vec<String> {
        match self {
            Self::Cuda(a) => a.decoder_arg(),
            Self::Qsv(a) => a.decoder_arg(),
            Self::Vaapi(a) => a.decoder_arg(),
            Self::VideoToolbox(a) => a.decoder_arg(),
        }
    }

    fn decoder_filters(&self) -> Vec<PipelineFilter> {
        match self {
            Self::Cuda(a) => a.decoder_filters(),
            Self::Qsv(a) => a.decoder_filters(),
            Self::Vaapi(a) => a.decoder_filters(),
            Self::VideoToolbox(a) => a.decoder_filters(),
        }
    }

    fn envs(&self) -> Vec<(String, String)> {
        match self {
            Self::Cuda(a) => a.envs(),
            Self::Qsv(a) => a.envs(),
            Self::Vaapi(a) => a.envs(),
            Self::VideoToolbox(a) => a.envs(),
        }
    }

    fn ffmpeg_name(&self) -> &str {
        match self {
            Self::Cuda(a) => a.ffmpeg_name(),
            Self::Qsv(a) => a.ffmpeg_name(),
            Self::Vaapi(a) => a.ffmpeg_name(),
            Self::VideoToolbox(a) => a.ffmpeg_name(),
        }
    }

    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter> {
        match self {
            Self::Cuda(a) => a.format_filter(pixel_format),
            Self::Qsv(a) => a.format_filter(pixel_format),
            Self::Vaapi(a) => a.format_filter(pixel_format),
            Self::VideoToolbox(a) => a.format_filter(pixel_format),
        }
    }

    fn frame_surface(&self) -> FrameSurface {
        match self {
            Self::Cuda(a) => a.frame_surface(),
            Self::Qsv(a) => a.frame_surface(),
            Self::Vaapi(a) => a.frame_surface(),
            Self::VideoToolbox(a) => a.frame_surface(),
        }
    }

    fn init_hw_device(&self) -> Vec<String> {
        match self {
            Self::Cuda(a) => a.init_hw_device(),
            Self::Qsv(a) => a.init_hw_device(),
            Self::Vaapi(a) => a.init_hw_device(),
            Self::VideoToolbox(a) => a.init_hw_device(),
        }
    }

    fn output_format(&self, source_pixel_format: &PixelFormat) -> PixelFormat {
        match self {
            Self::Cuda(a) => a.output_format(source_pixel_format),
            Self::Qsv(a) => a.output_format(source_pixel_format),
            Self::Vaapi(a) => a.output_format(source_pixel_format),
            Self::VideoToolbox(a) => a.output_format(source_pixel_format),
        }
    }

    fn supports_pixel_format(&self, pixel_format: &PixelFormat) -> bool {
        match self {
            Self::Qsv(a) => a.supports_pixel_format(pixel_format),
            Self::Vaapi(a) => a.supports_pixel_format(pixel_format),
            _ => true,
        }
    }
}
