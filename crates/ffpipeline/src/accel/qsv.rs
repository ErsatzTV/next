use crate::capabilities::qsv::QsvCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::hw_accel::HwAccel;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::{HwVideoFilter, VideoFilter};

#[derive(Debug, Clone)]
pub struct Qsv {
    pub capabilities: QsvCapabilities,
}

impl HwAccel for Qsv {
    fn best_filter(
        &self,
        video_filter: &VideoFilter,
        ffmpeg_info: &FfmpegInfo,
        _current_state: &FrameState,
    ) -> VideoFilter {
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

    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        let format = match codec {
            "h264" => Some(VideoFormat::H264),
            "hevc" => Some(VideoFormat::Hevc),
            _ => None,
        };

        if let Some(format) = format {
            self.capabilities
                .can_decode(&format, pixel_format.bit_depth())
        } else {
            false
        }
    }

    fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        self.capabilities.can_encode(format, bit_depth)
    }

    fn codec_for_format(&self, format: &VideoFormat) -> Option<VideoCodec> {
        match format {
            VideoFormat::H264 => Some(VideoCodec {
                codec_name: "h264_qsv",
                options: &["-low_power", "0", "-look_ahead", "0"],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                is_hardware: true,
            }),
            VideoFormat::Hevc => Some(VideoCodec {
                codec_name: "hevc_qsv",
                options: &["-low_power", "0", "-look_ahead", "0"],
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
            String::from("qsv"),
            String::from("-hwaccel_output_format"),
            String::from("qsv"),
        ]
    }

    fn decoder_frame_surface(&self) -> FrameSurface {
        FrameSurface::Qsv
    }

    fn encoder_frame_surface(&self) -> FrameSurface {
        FrameSurface::Qsv
    }

    fn initialize(&self, _ffmpeg_info: &FfmpegInfo, _is_hdr: bool) -> Self {
        self.clone()
    }

    fn init_hw_device(&self) -> Vec<String> {
        vec![
            String::from("-init_hw_device"),
            String::from("qsv=hw"),
            String::from("-filter_hw_device"),
            String::from("hw"),
        ]
    }

    fn known_accel(&self) -> &KnownHardwareAccel {
        &KnownHardwareAccel::Qsv
    }

    fn supports_pixel_format(&self, pixel_format: &PixelFormat) -> bool {
        self.capabilities.vpp_supports_format(pixel_format)
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
