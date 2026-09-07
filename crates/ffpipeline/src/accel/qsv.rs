use serde::Serialize;

use crate::ArgVec;
use crate::capabilities::qsv::QsvCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::output_settings::VideoFilterOptions;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, SurfaceSet, VideoFormat};
use crate::probe::ProbeResultVideoStream;
use crate::video_codec::VideoCodec;
use crate::video_filter::{DeinterlaceFilter, PadFilter, ScaleFilter, VideoFilter, VideoFilterOp};

const VPP_QSV_PAD_OPTION: &str = "pad_w";

#[derive(Debug, Clone, Serialize)]
pub struct Qsv {
    pub capabilities: QsvCapabilities,
}

impl HwAccel for Qsv {
    fn best_filter(
        &self,
        video_filter: &VideoFilter,
        ffmpeg_info: &FfmpegInfo,
        _current_state: &FrameState,
        filter_options: &VideoFilterOptions,
    ) -> VideoFilter {
        match video_filter {
            VideoFilter::Scale(ScaleFilter {
                size,
                input_is_anamorphic,
                ..
            }) if ffmpeg_info.has_video_filter(&KnownVideoFilter::VppQsv) => ScaleQsv {
                size: *size,
                input_is_anamorphic: *input_is_anamorphic,
            }
            .into(),
            VideoFilter::Deinterlace(DeinterlaceFilter { .. })
                if ffmpeg_info.has_video_filter(&KnownVideoFilter::DeinterlaceQsv) =>
            {
                DeinterlaceQsv {
                    mode: filter_options.deinterlace_qsv.mode.clone(),
                }
                .into()
            }
            // upstream vpp_qsv has no pad options, only the patched ErsatzTV builds do
            VideoFilter::Pad(PadFilter { size, .. })
                if ffmpeg_info
                    .has_video_filter_option(&KnownVideoFilter::VppQsv, VPP_QSV_PAD_OPTION) =>
            {
                PadQsv {
                    size: *size,
                    scale: None,
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

    fn codec_for_format(
        &self,
        format: &VideoFormat,
        _bit_depth: u8,
        _video_size: Option<FrameSize>,
    ) -> Option<VideoCodec> {
        match format {
            VideoFormat::H264 => Some(VideoCodec {
                codec_name: "h264_qsv",
                options: args!["-low_power", "0", "-look_ahead", "0", "-forced_idr", "1"],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::Qsv,
            }),
            VideoFormat::Hevc => Some(VideoCodec {
                codec_name: "hevc_qsv",
                options: args![
                    "-low_power",
                    "0",
                    "-look_ahead",
                    "0",
                    "-forced_idr",
                    "1",
                    "-tag:v",
                    "hvc1",
                ],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::Qsv,
            }),
            _ => None,
        }
    }

    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter> {
        Some(
            FormatQsv {
                format: *pixel_format,
            }
            .into(),
        )
    }

    fn init_hw_device(&self, _surfaces: &SurfaceSet) -> ArgVec {
        args!["-init_hw_device", "qsv=hw", "-filter_hw_device", "hw",]
    }

    fn known_accel(&self) -> Option<&KnownHardwareAccel> {
        Some(&KnownHardwareAccel::Qsv)
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
                args: args!["-hwaccel", "qsv", "-hwaccel_output_format", "qsv",],
                surface: FrameSurface::Qsv,
                filters: Vec::new(),
            })
        } else {
            None
        }
    }

    fn accepts_upload_format(&self, pixel_format: &PixelFormat) -> bool {
        self.capabilities.vpp_supports_format(pixel_format)
    }
}

#[derive(Debug, Clone)]
pub struct ScaleQsv {
    pub(crate) size: Option<FrameSize>,
    pub(crate) input_is_anamorphic: bool,
}

impl ScaleQsv {
    fn as_arg_with(&self, extra_options: &str) -> Option<String> {
        let size = self.size?;
        let prescale = if self.input_is_anamorphic {
            "vpp_qsv=w=iw*sar:h=ih,"
        } else {
            ""
        };
        Some(format!(
            "{prescale}vpp_qsv=w={}:h={}{extra_options},setsar=1",
            size.width, size.height
        ))
    }
}

impl VideoFilterOp for ScaleQsv {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::Qsv;
            state.is_anamorphic = false;
            state.sample_aspect_ratio = Some(String::from("1:1"));
            state.display_aspect_ratio = None;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Qsv)
    }

    fn as_arg(&self) -> Option<String> {
        self.as_arg_with("")
    }
}

#[derive(Debug, Clone)]
pub struct PadQsv {
    pub(crate) size: Option<FrameSize>,
    /// scale and pad share one vpp_qsv instance: one VPP pass, and two chained
    /// vpp_qsv instances were measured to drop the last frame at EOF
    pub(crate) scale: Option<ScaleQsv>,
}

impl PadQsv {
    fn pad_options(size: &FrameSize) -> String {
        format!(
            "pad_w={}:pad_h={}:pad_x=-1:pad_y=-1:pad_color=black",
            size.width, size.height
        )
    }
}

impl VideoFilterOp for PadQsv {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(scale) = &self.scale {
            scale.apply_to(state);
        }
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::Qsv;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Qsv)
    }

    fn as_arg(&self) -> Option<String> {
        let pad_options = Self::pad_options(self.size.as_ref()?);
        self.scale
            .as_ref()
            .and_then(|scale| scale.as_arg_with(&format!(":{pad_options}")))
            .or_else(|| Some(format!("vpp_qsv={pad_options}")))
    }
}

#[derive(Debug, Clone)]
pub struct FormatQsv {
    pub(crate) format: PixelFormat,
}

impl VideoFilterOp for FormatQsv {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format;
        state.surface = FrameSurface::Qsv;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Qsv)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!("vpp_qsv=format={}", self.format.as_arg()))
    }
}

#[derive(Debug, Clone)]
pub struct DeinterlaceQsv {
    pub mode: Option<String>,
}

impl VideoFilterOp for DeinterlaceQsv {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.is_interlaced = false;
        state.surface = FrameSurface::Qsv;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Qsv)
    }

    fn as_arg(&self) -> Option<String> {
        let mode = self.mode.as_deref().unwrap_or("2");
        Some(format!("deinterlace_qsv=mode={mode}"))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::filter_chain::{FilterChain, PipelineFilter};
    use crate::hw_accel::HardwareAccel;
    use crate::output_settings::ScalingMode;

    fn make_qsv() -> Qsv {
        Qsv {
            capabilities: QsvCapabilities {
                supported_decoders: HashMap::new(),
                supported_encoders: HashMap::new(),
                vpp_pixel_formats: HashSet::new(),
            },
        }
    }

    fn make_ffmpeg_info(with_pad_option: bool) -> FfmpegInfo {
        let mut video_filters = HashSet::new();
        video_filters.insert(KnownVideoFilter::VppQsv.to_string());

        let mut video_filter_options = HashMap::new();
        let mut options = HashSet::from([String::from("deinterlace"), String::from("denoise")]);
        if with_pad_option {
            options.extend([
                String::from("pad_w"),
                String::from("pad_h"),
                String::from("pad_x"),
                String::from("pad_y"),
                String::from("pad_color"),
            ]);
        }
        video_filter_options.insert(KnownVideoFilter::VppQsv.to_string(), options);

        FfmpegInfo {
            hwaccels: HashSet::new(),
            video_filters,
            preferred_filters: HashMap::new(),
            video_filter_options,
        }
    }

    fn make_frame_state() -> FrameState {
        FrameState {
            size: FrameSize {
                width: 1440,
                height: 1080,
            },
            is_anamorphic: false,
            is_interlaced: false,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            surface: FrameSurface::Qsv,
            pixel_format: PixelFormat::Nv12,
            is_hdr: false,
        }
    }

    fn pad_1920x1080() -> VideoFilter {
        VideoFilter::Pad(PadFilter {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
            scaling_mode: ScalingMode::ScaleAndPad,
        })
    }

    #[test]
    fn best_filter_selects_pad_qsv_when_vpp_qsv_has_pad_option() {
        let qsv = make_qsv();
        let ffmpeg_info = make_ffmpeg_info(true);
        let state = make_frame_state();
        let filter_options = VideoFilterOptions::default();

        let result = qsv.best_filter(&pad_1920x1080(), &ffmpeg_info, &state, &filter_options);

        match result {
            VideoFilter::PadQsv(PadQsv {
                size: Some(size),
                scale: None,
            }) => {
                assert_eq!(size.width, 1920);
                assert_eq!(size.height, 1080);
            }
            other => panic!("expected PadQsv, got {other:?}"),
        }
    }

    #[test]
    fn best_filter_falls_back_to_software_pad_without_pad_option() {
        let qsv = make_qsv();
        let ffmpeg_info = make_ffmpeg_info(false);
        let state = make_frame_state();
        let filter_options = VideoFilterOptions::default();

        let result = qsv.best_filter(&pad_1920x1080(), &ffmpeg_info, &state, &filter_options);

        assert!(
            matches!(result, VideoFilter::Pad(_)),
            "expected software Pad fallback, got {result:?}"
        );
    }

    #[test]
    fn pad_qsv_arg_and_state() {
        let pad = PadQsv {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
            scale: None,
        };

        assert_eq!(
            pad.as_arg().as_deref(),
            Some("vpp_qsv=pad_w=1920:pad_h=1080:pad_x=-1:pad_y=-1:pad_color=black")
        );

        let mut state = make_frame_state();
        pad.apply_to(&mut state);
        assert_eq!(state.size.width, 1920);
        assert_eq!(state.size.height, 1080);
        assert_eq!(state.surface, FrameSurface::Qsv);
    }

    #[test]
    fn pad_qsv_fused_scale_emits_single_instance() {
        let pad = PadQsv {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
            scale: Some(ScaleQsv {
                size: Some(FrameSize {
                    width: 1440,
                    height: 1080,
                }),
                input_is_anamorphic: false,
            }),
        };

        assert_eq!(
            pad.as_arg().as_deref(),
            Some(
                "vpp_qsv=w=1440:h=1080:pad_w=1920:pad_h=1080:pad_x=-1:pad_y=-1:pad_color=black,setsar=1"
            )
        );

        let mut state = make_frame_state();
        state.is_anamorphic = true;
        pad.apply_to(&mut state);
        assert_eq!(state.size.width, 1920);
        assert_eq!(state.size.height, 1080);
        assert!(!state.is_anamorphic);
        assert_eq!(state.sample_aspect_ratio.as_deref(), Some("1:1"));
    }

    #[test]
    fn pad_qsv_fused_anamorphic_scale_prescales_then_combines() {
        let pad = PadQsv {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
            scale: Some(ScaleQsv {
                size: Some(FrameSize {
                    width: 1440,
                    height: 1080,
                }),
                input_is_anamorphic: true,
            }),
        };

        assert_eq!(
            pad.as_arg().as_deref(),
            Some(
                "vpp_qsv=w=iw*sar:h=ih,vpp_qsv=w=1440:h=1080:pad_w=1920:pad_h=1080:pad_x=-1:pad_y=-1:pad_color=black,setsar=1"
            )
        );
    }

    #[test]
    fn scale_qsv_arg() {
        let size = Some(FrameSize {
            width: 1440,
            height: 1080,
        });

        let scale = ScaleQsv {
            size,
            input_is_anamorphic: false,
        };
        assert_eq!(
            scale.as_arg().as_deref(),
            Some("vpp_qsv=w=1440:h=1080,setsar=1")
        );

        let anamorphic = ScaleQsv {
            size,
            input_is_anamorphic: true,
        };
        assert_eq!(
            anamorphic.as_arg().as_deref(),
            Some("vpp_qsv=w=iw*sar:h=ih,vpp_qsv=w=1440:h=1080,setsar=1")
        );
    }

    #[test]
    fn optimize_fuses_scale_then_pad_into_single_vpp_qsv() {
        let accel = HardwareAccel::Qsv(make_qsv());
        let ffmpeg_info = make_ffmpeg_info(true);
        let filter_options = VideoFilterOptions::default();
        let initial_state = make_frame_state();

        let scale: VideoFilter = ScaleFilter {
            size: Some(FrameSize {
                width: 1440,
                height: 1080,
            }),
            scaling_mode: ScalingMode::ScaleAndPad,
            input_is_anamorphic: false,
            force_original_aspect_ratio: None,
        }
        .into();

        let mut chain = FilterChain::new(vec![
            PipelineFilter::Video(scale),
            PipelineFilter::Video(pad_1920x1080()),
        ]);

        chain.resolve(
            &ffmpeg_info,
            &Some(accel),
            &filter_options,
            &initial_state,
            &FrameSurface::Qsv,
            &Some(PixelFormat::Nv12),
        );
        chain.optimize();
        chain.build("0:a", "0:v", None, None);

        let args = chain.as_arg();
        let filter_complex = &args[1];

        assert!(
            filter_complex.contains(
                "vpp_qsv=w=1440:h=1080:pad_w=1920:pad_h=1080:pad_x=-1:pad_y=-1:pad_color=black"
            ),
            "expected a single combined vpp_qsv scale+pad: {filter_complex}"
        );
        assert_eq!(
            filter_complex.matches("vpp_qsv").count(),
            1,
            "scale+pad must collapse to exactly one vpp_qsv: {filter_complex}"
        );
    }

    #[test]
    fn optimize_leaves_pad_only_when_scale_is_not_adjacent() {
        let pad: VideoFilter = PadQsv {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
            scale: None,
        }
        .into();

        let mut chain = FilterChain::new(vec![PipelineFilter::Video(pad)]);
        chain.optimize();
        chain.build("0:a", "0:v", None, None);

        let args = chain.as_arg();
        assert!(
            args[1].contains("vpp_qsv=pad_w=1920:pad_h=1080:pad_x=-1:pad_y=-1:pad_color=black"),
            "expected a pad-only vpp_qsv: {}",
            args[1]
        );
    }
}
