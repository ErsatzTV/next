use crate::ArgVec;
use crate::accel::opencl::{PadOpencl, TonemapOpencl};
use crate::capabilities::opencl::OpenCLCapabilities;
use crate::capabilities::vaapi::{RateControlMode, VaapiCapabilities};
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::output_settings::VideoFilterOptions;
use crate::overlay_filter::{FramePoint, OverlayFilter, OverlayKind, OverlayKindOp};
use crate::pipeline::{
    EnvironmentVariable, FrameState, FrameSurface, HwPixelFormat, PixelFormat, SurfaceSet,
    VideoFormat,
};
use crate::probe::ProbeResultVideoStream;
use crate::video_codec::VideoCodec;
use crate::video_filter::{
    DeinterlaceFilter, ForceOriginalAspectRatio, HwMapFilter, PadFilter, ScaleFilter,
    ToneMapFilter, VideoFilter, VideoFilterOp,
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
        filter_options: &VideoFilterOptions,
    ) -> VideoFilter {
        match video_filter {
            VideoFilter::Scale(ScaleFilter {
                size,
                input_is_anamorphic,
                force_original_aspect_ratio,
                ..
            }) if ffmpeg_info.has_video_filter(&KnownVideoFilter::ScaleVaapi)
                && !current_state.pixel_format.has_alpha()
                && self
                    .capabilities
                    .vpp_supports_format(&current_state.pixel_format) =>
            {
                ScaleVaapi {
                    size: *size,
                    input_is_anamorphic: *input_is_anamorphic,
                    force_original_aspect_ratio: force_original_aspect_ratio.clone(),
                }
                .into()
            }
            VideoFilter::Pad(PadFilter { size, .. }) => {
                let mut pad_options = vec![KnownVideoFilter::PadVaapi];
                if self.opencl_capabilities.can_pad() {
                    pad_options.push(KnownVideoFilter::PadOpencl);
                }
                if let Some(hw_filter) = ffmpeg_info.find_best_fit(pad_options.as_slice()) {
                    match hw_filter {
                        KnownVideoFilter::PadVaapi
                            if self
                                .capabilities
                                .vpp_supports_format(&current_state.pixel_format) =>
                        {
                            PadVaapi { size: *size }.into()
                        }
                        KnownVideoFilter::PadOpencl => PadOpencl { size: *size }.into(),
                        _ => video_filter.clone(),
                    }
                } else {
                    video_filter.clone()
                }
            }

            VideoFilter::Deinterlace(DeinterlaceFilter {
                input_is_interlaced,
                ..
            }) if *input_is_interlaced
                && ffmpeg_info.has_video_filter(&KnownVideoFilter::DeinterlaceVaapi)
                && self
                    .capabilities
                    .vpp_supports_format(&current_state.pixel_format) =>
            {
                DeinterlaceVaapi {
                    mode: filter_options.deinterlace_vaapi.mode.clone(),
                }
                .into()
            }

            VideoFilter::ToneMap(ToneMapFilter {
                output_format: format,
                ..
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
                            algorithm: filter_options.tonemap_opencl.tonemap.clone(),
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

    fn best_overlay(
        &self,
        overlay_filter: &OverlayFilter,
        ffmpeg_info: &FfmpegInfo,
        _current_state: &FrameState,
    ) -> OverlayFilter {
        match overlay_filter.kind {
            OverlayKind::Software(_)
                if ffmpeg_info.has_video_filter(&KnownVideoFilter::OverlayVaapi)
                    && self.capabilities.can_overlay
                    && self.capabilities.vpp_supports_format(&PixelFormat::Bgra) =>
            {
                overlay_filter.with_kind(OverlayKind::Vaapi(VaapiOverlay))
            }
            _ => overlay_filter.clone(),
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
        bit_depth: u8,
        video_size: Option<FrameSize>,
    ) -> Option<VideoCodec> {
        let force_cqp = self.capabilities.rate_control_mode_for(format, bit_depth)
            == Some(RateControlMode::Cqp);

        match format {
            VideoFormat::H264 => {
                let options = if force_cqp {
                    args!["-rc_mode", "1"]
                } else {
                    Vec::new()
                };

                Some(VideoCodec {
                    codec_name: "h264_vaapi",
                    options,
                    preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                    preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                    preferred_surface: FrameSurface::Vaapi,
                })
            }
            VideoFormat::Hevc => {
                let mut options = Vec::new();
                if force_cqp {
                    options.extend(args!["-rc_mode", "1"]);
                }

                // WORKAROUND: RadeonSI doesn't always output appropriate crop
                // metadata with HEVC encoder; it doesn't hurt anything to always specify
                if self.driver == VaapiDriver::RadeonSI
                    && video_size.map(|s| s.height) == Some(1080)
                {
                    options.extend(args!["-bsf:v", "hevc_metadata=crop_bottom=8"]);
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

    fn envs(&self) -> Vec<EnvironmentVariable> {
        vec![EnvironmentVariable {
            key: String::from("LIBVA_DRIVER_NAME"),
            value: self.driver.to_string(),
        }]
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
                "va",
                "-filter_hw_device",
                "va"
            ]
        } else {
            args!["-vaapi_device", self.device.clone()]
        }
    }

    fn known_accel(&self) -> Option<&KnownHardwareAccel> {
        Some(&KnownHardwareAccel::Vaapi)
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
                args: args![
                    "-hwaccel",
                    KnownHardwareAccel::Vaapi,
                    "-hwaccel_output_format",
                    KnownHardwareAccel::Vaapi,
                ],
                surface: FrameSurface::Vaapi,
                filters: Vec::new(),
            })
        } else {
            None
        }
    }

    fn accepts_upload_format(&self, pixel_format: &PixelFormat) -> bool {
        // upload works even when vpp is unsupported for a format, so match
        // the canonical VA surface formats first.
        // it is safe to allow 10 bit here because encoder is already checked,
        // e.g. 10-bit software encoder will never try to upload
        matches!(pixel_format, PixelFormat::Nv12 | PixelFormat::P010le)
            || self.capabilities.vpp_supports_format(pixel_format)
    }

    fn can_convert_pixel_format(&self, pixel_format: &PixelFormat) -> bool {
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

#[derive(Clone)]
pub struct VaapiOverlay;

impl OverlayKindOp for VaapiOverlay {
    fn apply_to(&self, state: &mut FrameState) {
        state.surface = FrameSurface::Vaapi;
    }

    fn main_input_state(&self, current_state: &FrameState) -> FrameState {
        FrameState {
            surface: FrameSurface::Vaapi,
            ..current_state.clone()
        }
    }

    fn secondary_input_state(&self, current_state: &FrameState) -> FrameState {
        FrameState {
            pixel_format: PixelFormat::Bgra,
            surface: FrameSurface::Vaapi,
            ..current_state.clone()
        }
    }

    fn as_arg(&self, location: Option<FramePoint>) -> Option<String> {
        if let Some(location) = location {
            Some(format!("overlay_vaapi=x={}:y={}", location.x, location.y))
        } else {
            Some(String::from("overlay_vaapi=x=(W-w)/2:y=(H-h)/2"))
        }
    }
}

#[derive(Clone)]
pub struct DeinterlaceVaapi {
    pub mode: Option<String>,
}

impl VideoFilterOp for DeinterlaceVaapi {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.is_interlaced = false;
        state.surface = FrameSurface::Vaapi;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Vaapi)
    }

    fn as_arg(&self) -> Option<String> {
        let mode = self.mode.as_deref().unwrap_or("0");
        Some(format!("deinterlace_vaapi=mode={mode}"))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::ffmpeg_info::FfmpegInfo;
    use crate::frame_size::FrameSize;
    use crate::output_settings::YadifOptions;
    use crate::pipeline::{FrameState, FrameSurface, PixelFormat};
    use crate::video_filter::{
        DeinterlaceFilter, SoftwareDeinterlaceFilter, SoftwareDeinterlaceOptions,
    };

    fn make_ffmpeg_info(filters: &[KnownVideoFilter]) -> FfmpegInfo {
        let mut video_filters = HashSet::new();
        for f in filters {
            video_filters.insert(f.to_string());
        }
        FfmpegInfo {
            hwaccels: HashSet::new(),
            video_filters,
            preferred_filters: HashMap::new(),
        }
    }

    fn make_vaapi() -> Vaapi {
        Vaapi {
            device: String::from("/dev/dri/renderD128"),
            driver: VaapiDriver::Ihd,
            capabilities: VaapiCapabilities {
                vendor: String::from("test"),
                supported: HashSet::new(),
                vpp_pixel_formats: HashSet::from([libva_sys::VA_FOURCC_NV12]),
                can_hdr_to_sdr_tonemap: HashSet::new(),
                can_hdr_to_hdr_tonemap: HashSet::new(),
                can_overlay: false,
                rate_control: HashMap::new(),
            },
            opencl_capabilities: OpenCLCapabilities {
                platform_count: 0,
                gpu_device_count: 0,
            },
        }
    }

    fn make_frame_state() -> FrameState {
        FrameState {
            size: FrameSize {
                width: 1920,
                height: 1080,
            },
            is_anamorphic: false,
            is_interlaced: false,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            surface: FrameSurface::Vaapi,
            pixel_format: PixelFormat::Nv12,
            is_hdr: false,
        }
    }

    #[test]
    fn best_filter_deinterlace_vaapi_for_interlaced_anamorphic_content() {
        let vaapi = make_vaapi();
        let ffmpeg_info = make_ffmpeg_info(&[KnownVideoFilter::DeinterlaceVaapi]);
        let state = FrameState {
            is_anamorphic: true,
            is_interlaced: true,
            sample_aspect_ratio: Some(String::from("32:27")),
            display_aspect_ratio: Some(String::from("16:9")),
            size: FrameSize {
                width: 1440,
                height: 1080,
            },
            ..make_frame_state()
        };
        let filter_options = VideoFilterOptions::default();

        let sw_deinterlace = VideoFilter::Deinterlace(DeinterlaceFilter {
            filter: SoftwareDeinterlaceFilter::Yadif(YadifOptions::default()),
            options: SoftwareDeinterlaceOptions::default(),
            input_is_interlaced: true,
        });

        let result = vaapi.best_filter(&sw_deinterlace, &ffmpeg_info, &state, &filter_options);

        assert!(
            matches!(
                &result,
                VideoFilter::DeinterlaceVaapi(DeinterlaceVaapi { mode: None })
            ),
            "expected DeinterlaceVaapi with Default mode, got {:?}",
            result.as_arg()
        );
        assert_eq!(
            result.as_arg(),
            Some(String::from("deinterlace_vaapi=mode=0"))
        );
    }

    #[test]
    fn deinterlace_vaapi_apply_to_clears_interlaced_keeps_anamorphic() {
        let mut state = FrameState {
            is_anamorphic: true,
            is_interlaced: true,
            sample_aspect_ratio: Some(String::from("32:27")),
            display_aspect_ratio: Some(String::from("16:9")),
            size: FrameSize {
                width: 1440,
                height: 1080,
            },
            ..make_frame_state()
        };

        let filter = DeinterlaceVaapi { mode: None };
        filter.apply_to(&mut state);

        assert!(!state.is_interlaced);
        assert!(state.is_anamorphic);
        assert_eq!(state.surface, FrameSurface::Vaapi);
        assert_eq!(state.sample_aspect_ratio, Some(String::from("32:27")));
    }

    #[test]
    fn best_filter_falls_back_when_deinterlace_vaapi_unavailable() {
        let vaapi = make_vaapi();
        let ffmpeg_info = make_ffmpeg_info(&[]);
        let state = FrameState {
            is_anamorphic: true,
            is_interlaced: true,
            ..make_frame_state()
        };
        let filter_options = VideoFilterOptions::default();

        let sw_deinterlace = VideoFilter::Deinterlace(DeinterlaceFilter {
            filter: SoftwareDeinterlaceFilter::Yadif(YadifOptions::default()),
            options: SoftwareDeinterlaceOptions::default(),
            input_is_interlaced: true,
        });

        let result = vaapi.best_filter(&sw_deinterlace, &ffmpeg_info, &state, &filter_options);

        assert!(
            matches!(&result, VideoFilter::Deinterlace(_)),
            "expected software Deinterlace fallback, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn best_filter_skips_deinterlace_vaapi_when_not_interlaced() {
        let vaapi = make_vaapi();
        let ffmpeg_info = make_ffmpeg_info(&[KnownVideoFilter::DeinterlaceVaapi]);
        let state = make_frame_state();
        let filter_options = VideoFilterOptions::default();

        let sw_deinterlace = VideoFilter::Deinterlace(DeinterlaceFilter {
            filter: SoftwareDeinterlaceFilter::Yadif(YadifOptions::default()),
            options: SoftwareDeinterlaceOptions::default(),
            input_is_interlaced: false,
        });

        let result = vaapi.best_filter(&sw_deinterlace, &ffmpeg_info, &state, &filter_options);

        assert!(
            matches!(&result, VideoFilter::Deinterlace(_)),
            "expected software Deinterlace (not promoted), got {:?}",
            result.as_arg()
        );
    }
}
