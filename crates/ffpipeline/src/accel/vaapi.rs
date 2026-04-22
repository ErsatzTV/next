use std::fmt::{Display, Formatter};

use crate::ArgVec;
use crate::accel::opencl::TonemapOpencl;
use crate::capabilities::vaapi::VaapiCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::hw_accel::HwAccel;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, VideoFormat};
use crate::video_codec::VideoCodec;
use crate::video_filter::{
    ForceOriginalAspectRatio, HwMapFilter, PadFilter, ScaleFilter, ToneMapFilter, VideoFilter,
    VideoFilterOp,
};

#[derive(Debug, Clone, PartialEq)]
pub enum VaapiDriver {
    Ihd,
    I965,
    RadeonSI,
}

impl Display for VaapiDriver {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VaapiDriver::Ihd => write!(f, "iHD"),
            VaapiDriver::I965 => write!(f, "i965"),
            VaapiDriver::RadeonSI => write!(f, "radeonsi"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Vaapi {
    pub device: String,
    pub driver: VaapiDriver,
    pub capabilities: VaapiCapabilities,
    pub needs_opencl_device: bool,
}

impl HwAccel for Vaapi {
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
                force_original_aspect_ratio,
            }) if ffmpeg_info.has_video_filter(&KnownVideoFilter::ScaleVaapi) => ScaleVaapi {
                size: *size,
                input_is_anamorphic: *input_is_anamorphic,
                force_original_aspect_ratio: force_original_aspect_ratio.clone(),
            }
            .into(),
            VideoFilter::Pad(PadFilter { size })
                if ffmpeg_info.has_video_filter(&KnownVideoFilter::PadVaapi) =>
            {
                PadVaapi { size: *size }.into()
            }

            VideoFilter::ToneMap(ToneMapFilter { algorithm, format }) => {
                if let Some(hw_filter) = ffmpeg_info.find_best_fit(&[
                    KnownVideoFilter::TonemapOpencl,
                    KnownVideoFilter::TonemapVaapi,
                ]) {
                    match hw_filter {
                        KnownVideoFilter::TonemapOpencl => TonemapOpencl {
                            algorithm: algorithm.clone(),
                            output_format: match format.bit_depth() {
                                10 => PixelFormat::P010le,
                                _ => PixelFormat::Nv12,
                            },
                        }
                        .into(),
                        // TODO: Implement tonemap vaapi
                        _ => video_filter.clone(),
                    }
                } else {
                    video_filter.clone()
                }
            }

            _ => video_filter.clone(),
        }
    }

    fn can_decode(&self, codec: &str, profile: &str, pixel_format: &PixelFormat) -> bool {
        let result = self
            .capabilities
            .can_decode(codec, profile, pixel_format.bit_depth());

        if !result {
            log::debug!(
                "VAAPI does not support decoding {}/{}, will use software decoder",
                codec,
                profile
            );
        }

        result
    }

    fn can_encode(&self, format: &VideoFormat, bit_depth: u8) -> bool {
        let result = self.capabilities.can_encode(format, bit_depth)
            || self.capabilities.can_encode_low_power(format, bit_depth);

        if !result {
            log::debug!(
                "VAAPI does not support encoding {}-bit {:?}, will use software encoder",
                bit_depth,
                format,
            );
        }

        result
    }

    fn codec_for_format(
        &self,
        format: &VideoFormat,
        video_size: Option<FrameSize>,
    ) -> Option<VideoCodec> {
        match format {
            VideoFormat::H264 => Some(VideoCodec {
                codec_name: "h264_vaapi",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::Vaapi,
            }),
            VideoFormat::Hevc => {
                let mut options: &'static [&'static str] = &[];

                // WORKAROUND: RadeonSI doesn't always output appropriate crop
                // metadata with HEVC encoder; it doesn't hurt anything to always specify
                if self.driver == VaapiDriver::RadeonSI
                    && video_size.map(|s| s.height) == Some(1080)
                {
                    options = &["-bsf:v", "hevc_metadata=crop_bottom=8"];
                }

                Some(VideoCodec {
                    codec_name: "hevc_vaapi",
                    options,
                    preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                    preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                    preferred_surface: FrameSurface::Vaapi,
                })
            }
            _ => None,
        }
    }

    fn decoder_arg(&self) -> ArgVec {
        if self.needs_opencl_device {
            return args![
                "-hwaccel",
                KnownHardwareAccel::Vaapi,
                "-hwaccel_output_format",
                KnownHardwareAccel::Vaapi,
                "-hwaccel_device",
                "va",
            ];
        }

        args![
            "-hwaccel",
            KnownHardwareAccel::Vaapi,
            "-hwaccel_output_format",
            KnownHardwareAccel::Vaapi,
        ]
    }

    fn decoder_frame_surface(&self) -> FrameSurface {
        FrameSurface::Vaapi
    }

    fn envs(&self) -> Vec<(String, String)> {
        vec![(String::from("LIBVA_DRIVER_NAME"), self.driver.to_string())]
    }

    fn hw_map_filter(&self, from: &FrameSurface, to: &FrameSurface) -> Option<VideoFilter> {
        match (from, to) {
            (FrameSurface::Vaapi, FrameSurface::OpenCL) => Some(
                HwMapFilter {
                    from_surface: *from,
                    to_surface: *to,
                    reverse: false,
                }
                .into(),
            ),
            (FrameSurface::OpenCL, FrameSurface::Vaapi) => Some(
                HwMapFilter {
                    from_surface: *from,
                    to_surface: *to,
                    reverse: true,
                }
                .into(),
            ),
            _ => None,
        }
    }

    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter> {
        Some(
            FormatVaapi {
                format: pixel_format.clone(),
            }
            .into(),
        )
    }

    fn initialize(&self, ffmpeg_info: &FfmpegInfo, is_hdr: bool) -> Self {
        Vaapi {
            device: self.device.clone(),
            driver: self.driver.clone(),
            capabilities: self.capabilities.clone(),
            // Logic is a bit disjoint. It would be better if "best" filter could
            // append state around the pipeline.
            needs_opencl_device: is_hdr
                && ffmpeg_info
                    .find_best_fit(&[
                        KnownVideoFilter::TonemapOpencl,
                        KnownVideoFilter::TonemapVaapi,
                    ])
                    .is_some_and(|f| f == &KnownVideoFilter::TonemapOpencl),
        }
    }

    fn init_hw_device(&self) -> ArgVec {
        if self.needs_opencl_device {
            args![
                "-init_hw_device",
                format!("vaapi=va:{}", self.device.clone()),
                "-init_hw_device",
                "opencl=ocl@va"
            ]
        } else {
            args!["-vaapi_device", self.device.clone()]
        }
    }

    fn known_accel(&self) -> &KnownHardwareAccel {
        &KnownHardwareAccel::Vaapi
    }

    fn supports_pixel_format(&self, pixel_format: &PixelFormat) -> bool {
        self.capabilities.vpp_supports_format(pixel_format)
    }
}

#[derive(Clone)]
pub struct ScaleVaapi {
    pub(crate) size: Option<FrameSize>,
    pub(crate) input_is_anamorphic: bool,
    pub(crate) force_original_aspect_ratio: Option<ForceOriginalAspectRatio>,
}

impl VideoFilterOp for ScaleVaapi {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::Vaapi;
            state.is_anamorphic = false;
            state.sample_aspect_ratio = Some(String::from("1:1"));
            state.display_aspect_ratio = None;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Vaapi)
    }

    fn as_arg(&self) -> Option<String> {
        if let Some(size) = &self.size {
            let aspect_ratio = self
                .force_original_aspect_ratio
                .as_ref()
                .map_or(String::new(), |f| f.as_arg());

            if self.input_is_anamorphic {
                Some(format!(
                    "scale_vaapi=iw*sar:ih,setsar=1,scale_vaapi={}:{}{}:force_divisible_by=2",
                    size.width, size.height, aspect_ratio
                ))
            } else {
                Some(format!(
                    "scale_vaapi={}:{}{}:force_divisible_by=2,setsar=1",
                    size.width, size.height, aspect_ratio
                ))
            }
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct PadVaapi {
    pub(crate) size: Option<FrameSize>,
}

impl VideoFilterOp for PadVaapi {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::Vaapi;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Vaapi)
    }

    fn as_arg(&self) -> Option<String> {
        self.size
            .as_ref()
            .map(|s| format!("pad_vaapi={}:{}:-1:-1:color=black", s.width, s.height))
    }
}

#[derive(Clone)]
pub struct FormatVaapi {
    pub(crate) format: PixelFormat,
}

impl VideoFilterOp for FormatVaapi {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format.clone();
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Vaapi)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!("scale_vaapi=format={}", self.format.as_arg()))
    }
}
