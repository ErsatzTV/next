use crate::ArgVec;
use crate::capabilities::videotoolbox::VideoToolboxCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::hw_accel::HwAccel;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::{ScaleFilter, VideoFilter, VideoFilterOp};

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
    fn best_filter(
        &self,
        video_filter: &VideoFilter,
        ffmpeg_info: &FfmpegInfo,
        _current_state: &FrameState,
    ) -> VideoFilter {
        match video_filter {
            VideoFilter::Scale(ScaleFilter {
                size,
                input_is_anamorphic,
                ..
            }) if ffmpeg_info.has_video_filter(&KnownVideoFilter::ScaleVt) => ScaleVt {
                size: *size,
                input_is_anamorphic: *input_is_anamorphic,
            }
            .into(),
            _ => video_filter.clone(),
        }
    }

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

#[derive(Clone)]
pub struct ScaleVt {
    pub(crate) size: Option<FrameSize>,
    pub(crate) input_is_anamorphic: bool,
}

impl VideoFilterOp for ScaleVt {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::VideoToolbox;
            state.is_anamorphic = false;
            state.sample_aspect_ratio = Some(String::from("1:1"));
            state.display_aspect_ratio = None;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::VideoToolbox)
    }

    fn as_arg(&self) -> Option<String> {
        if let Some(size) = &self.size {
            if self.input_is_anamorphic {
                Some(format!(
                    "scale_vt=iw*sar:ih,scale_vt={}:{},setsar=1",
                    size.width, size.height
                ))
            } else {
                Some(format!("scale_vt={}:{},setsar=1", size.width, size.height))
            }
        } else {
            None
        }
    }
}
