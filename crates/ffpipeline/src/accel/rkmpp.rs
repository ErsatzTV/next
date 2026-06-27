use serde::Serialize;

use crate::ArgVec;
use crate::capabilities::rkmpp::RkmppCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::output_settings::VideoFilterOptions;
use crate::pipeline::{
    FrameState, FrameSurface, HwPixelFormat, PixelFormat, SurfaceSet, VideoFormat,
};
use crate::probe::ProbeResultVideoStream;
use crate::video_codec::VideoCodec;
use crate::video_filter::{ScaleFilter, VideoFilter, VideoFilterOp};

#[derive(Debug, Clone, Serialize)]
pub struct Rkmpp {
    pub capabilities: RkmppCapabilities,
}

impl HwAccel for Rkmpp {
    fn best_filter(
        &self,
        video_filter: &VideoFilter,
        ffmpeg_info: &FfmpegInfo,
        current_state: &FrameState,
        _filter_options: &VideoFilterOptions,
    ) -> VideoFilter {
        match video_filter {
            VideoFilter::Scale(ScaleFilter {
                size,
                input_is_anamorphic,
                ..
            }) if ffmpeg_info.has_video_filter(&KnownVideoFilter::ScaleRkrga)
                && current_state.pixel_format.bit_depth() == 8 =>
            {
                ScaleRkrga {
                    size: *size,
                    input_is_anamorphic: *input_is_anamorphic,
                }
                .into()
            }
            _ => video_filter.clone(),
        }
    }

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
        bit_depth: u8,
        _video_size: Option<FrameSize>,
    ) -> Option<VideoCodec> {
        match (format, bit_depth) {
            (VideoFormat::H264, 8) if self.capabilities.can_encode(format, 8) => Some(VideoCodec {
                codec_name: "h264_rkmpp",
                options: Vec::new(),
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: None,
                preferred_surface: FrameSurface::Rkmpp,
            }),
            (VideoFormat::Hevc, 8) if self.capabilities.can_encode(format, 8) => Some(VideoCodec {
                codec_name: "hevc_rkmpp",
                options: Vec::new(),
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: None,
                preferred_surface: FrameSurface::Rkmpp,
            }),
            _ => None,
        }
    }

    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter> {
        Some(
            FormatRkrga {
                format: *pixel_format,
            }
            .into(),
        )
    }

    fn init_hw_device(&self, _surfaces: &SurfaceSet) -> ArgVec {
        args!["-init_hw_device", "rkmpp=hw", "-filter_hw_device", "hw",]
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

    fn output_format(&self, source_pixel_format: &PixelFormat) -> HwPixelFormat {
        match source_pixel_format.bit_depth() {
            10 => HwPixelFormat::Nv15,
            _ => HwPixelFormat::Nv12,
        }
    }

    fn accepts_upload_format(&self, pixel_format: &PixelFormat) -> bool {
        pixel_format.bit_depth() == 8
    }

    fn can_convert_pixel_format(
        &self,
        ffmpeg_info: &FfmpegInfo,
        pixel_format: &PixelFormat,
    ) -> bool {
        ffmpeg_info.has_video_filter(&KnownVideoFilter::ScaleRkrga)
            && matches!(pixel_format, PixelFormat::Nv12 | PixelFormat::Nv15)
    }
}

#[derive(Debug, Clone)]
pub struct ScaleRkrga {
    pub(crate) size: Option<FrameSize>,
    pub(crate) input_is_anamorphic: bool,
}

impl VideoFilterOp for ScaleRkrga {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::Rkmpp;
            state.is_anamorphic = false;
            state.sample_aspect_ratio = Some(String::from("1:1"));
            state.display_aspect_ratio = None;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Rkmpp)
    }

    fn as_arg(&self) -> Option<String> {
        if let Some(size) = &self.size {
            if self.input_is_anamorphic {
                Some(format!(
                    "scale_rkrga=w=iw*sar:h=ih,scale_rkrga=w={}:h={}:force_original_aspect_ratio=0,setsar=1",
                    size.width, size.height
                ))
            } else {
                Some(format!(
                    "scale_rkrga=w={}:h={}:force_original_aspect_ratio=0,setsar=1",
                    size.width, size.height
                ))
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct FormatRkrga {
    pub(crate) format: PixelFormat,
}

impl VideoFilterOp for FormatRkrga {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format;
        state.surface = FrameSurface::Rkmpp;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Rkmpp)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!("scale_rkrga=format={}", self.format.as_arg()))
    }
}
