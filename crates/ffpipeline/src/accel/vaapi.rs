use crate::ArgVec;
use crate::accel::opencl::TonemapOpencl;
use crate::capabilities::opencl::OpenCLCapabilities;
use crate::capabilities::vaapi::VaapiCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::pipeline::{
    FrameState, FrameSurface, HwPixelFormat, PixelFormat, SurfaceSet, VideoFormat,
};
use crate::video_codec::VideoCodec;
use crate::video_filter::{
    ForceOriginalAspectRatio, HwMapFilter, PadFilter, ScaleFilter, ToneMapFilter, VideoFilter,
    VideoFilterOp,
};

#[derive(Debug, Clone, PartialEq, strum::Display)]
pub enum VaapiDriver {
    #[strum(serialize = "iHD")]
    Ihd,
    #[strum(serialize = "i965")]
    I965,
    #[strum(serialize = "radeonsi")]
    RadeonSI,
}

#[derive(Debug, Clone)]
pub struct Vaapi {
    pub device: String,
    pub driver: VaapiDriver,
    pub capabilities: VaapiCapabilities,
    pub opencl_capabilities: OpenCLCapabilities,
}

impl HwAccel for Vaapi {
    fn best_filter(
        &self,
        video_filter: &VideoFilter,
        ffmpeg_info: &FfmpegInfo,
        current_state: &FrameState,
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

            VideoFilter::ToneMap(ToneMapFilter {
                algorithm,
                output_format: format,
            }) => {
                let tonemap_output_format = self.output_format(format);
                let can_vaapi_tonemap = match (&current_state.pixel_format, tonemap_output_format) {
                    (fmt @ &PixelFormat::P010le, HwPixelFormat::P010le) => {
                        self.capabilities.can_hdr_to_hdr_tonemap(fmt)
                    }
                    (fmt @ &PixelFormat::P010le, HwPixelFormat::Nv12) => {
                        self.capabilities.can_hdr_to_sdr_tonemap(fmt)
                    }
                    _ => false,
                };

                let mut tonemap_options = vec![KnownVideoFilter::TonemapVaapi];
                if self.opencl_capabilities.can_tonemap() {
                    // Prepend because OpenCL is preferred.
                    tonemap_options.insert(0, KnownVideoFilter::TonemapOpencl);
                }
                if let Some(hw_filter) = ffmpeg_info.find_best_fit(tonemap_options.as_slice()) {
                    match hw_filter {
                        KnownVideoFilter::TonemapOpencl => TonemapOpencl {
                            algorithm: algorithm.clone(),
                            output_format: self.output_format(format),
                        }
                        .into(),
                        KnownVideoFilter::TonemapVaapi if can_vaapi_tonemap => TonemapVaapi {
                            output_format: self.output_format(format),
                        }
                        .into(),
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

    fn envs(&self) -> Vec<(String, String)> {
        vec![(String::from("LIBVA_DRIVER_NAME"), self.driver.to_string())]
    }

    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter> {
        Some(
            FormatVaapi {
                format: *pixel_format,
            }
            .into(),
        )
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

    fn init_hw_device(&self, surfaces: &SurfaceSet) -> ArgVec {
        if surfaces.contains(&FrameSurface::OpenCL) {
            args![
                "-init_hw_device",
                format!("vaapi=va:{}", self.device.clone()),
                "-init_hw_device",
                "opencl=ocl@va",
                "-hwaccel_device",
                "va"
            ]
        } else {
            args!["-vaapi_device", self.device.clone()]
        }
    }

    fn known_accel(&self) -> &KnownHardwareAccel {
        &KnownHardwareAccel::Vaapi
    }

    fn make_decoder(&self, _ffmpeg_info: &FfmpegInfo, _is_hdr: bool) -> HwDecoder {
        HwDecoder {
            args: args![
                "-hwaccel",
                KnownHardwareAccel::Vaapi,
                "-hwaccel_output_format",
                KnownHardwareAccel::Vaapi,
            ],
            surface: FrameSurface::Vaapi,
            filters: Vec::new(),
        }
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
                    "scale_vaapi=iw*sar:ih,scale_vaapi={}:{}{}:force_divisible_by=2,setsar=1",
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
        state.pixel_format = self.format;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Vaapi)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!("scale_vaapi=format={}", self.format.as_arg()))
    }
}

#[derive(Clone)]
pub struct TonemapVaapi {
    pub(crate) output_format: HwPixelFormat,
}

impl VideoFilterOp for TonemapVaapi {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.is_hdr = false;
        state.pixel_format = self.output_format.into();
        state.surface = FrameSurface::Vaapi;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Vaapi)
    }

    fn as_arg(&self) -> Option<String> {
        format!(
            "tonemap_vaapi=format={}:t=bt709:m=bt709:p=bt709",
            self.output_format.as_arg()
        )
        .into()
    }
}
