use serde::Serialize;

use crate::ArgVec;
use crate::capabilities::amf::AmfCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::filter_chain::PipelineFilter;
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::output_settings::VideoFilterOptions;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, SurfaceSet, VideoFormat};
use crate::probe::ProbeResultVideoStream;
use crate::video_codec::VideoCodec;
use crate::video_filter::{HwDownloadFilter, ScaleFilter, VideoFilter, VideoFilterOp};

#[derive(Debug, Clone, Serialize)]
pub struct Amf {
    pub capabilities: AmfCapabilities,
}

fn video_format(codec: &str) -> Option<VideoFormat> {
    match codec {
        "av1" => Some(VideoFormat::Av1),
        "h264" => Some(VideoFormat::H264),
        "hevc" => Some(VideoFormat::Hevc),
        "vp9" => Some(VideoFormat::Vp9),
        _ => None,
    }
}

impl HwAccel for Amf {
    fn best_filter(
        &self,
        video_filter: &VideoFilter,
        ffmpeg_info: &FfmpegInfo,
        _current_state: &FrameState,
        _filter_options: &VideoFilterOptions,
    ) -> VideoFilter {
        match video_filter {
            VideoFilter::Scale(ScaleFilter {
                size: Some(size), ..
            }) if ffmpeg_info.has_video_filter(&KnownVideoFilter::VppAmf) => {
                VppAmf::scale(*size).into()
            }
            _ => video_filter.clone(),
        }
    }

    /// Decoded surfaces go through vpp_amf for scaling and format conversion, so only
    /// decode on the device when the converter can consume the decoded surface format.
    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        video_format(codec).is_some_and(|f| {
            self.capabilities.can_decode(&f, pixel_format.bit_depth())
                && self.capabilities.vpp_accepts_input(pixel_format)
        })
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
        if !self.capabilities.can_encode(format, bit_depth) {
            return None;
        }

        match format {
            VideoFormat::H264 => Some(VideoCodec {
                codec_name: "h264_amf",
                options: Vec::new(),
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::Amf,
            }),
            VideoFormat::Hevc => Some(VideoCodec {
                codec_name: "hevc_amf",
                options: args!["-tag:v", "hvc1"],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::Amf,
            }),
            _ => None,
        }
    }

    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter> {
        if pixel_format.has_alpha() {
            None
        } else {
            Some(VppAmf::format(*pixel_format).into())
        }
    }

    fn init_hw_device(&self, _surfaces: &SurfaceSet) -> ArgVec {
        args!["-init_hw_device", "amf=hw", "-filter_hw_device", "hw"]
    }

    fn known_accel(&self) -> Option<&KnownHardwareAccel> {
        Some(&KnownHardwareAccel::Amf)
    }

    fn make_decoder(
        &self,
        _ffmpeg_info: &FfmpegInfo,
        video_stream: &ProbeResultVideoStream,
    ) -> Option<HwDecoder> {
        let pixel_format = PixelFormat::parse(&video_stream.pix_fmt);
        if !self.can_decode(&video_stream.codec, &video_stream.profile, &pixel_format) {
            return None;
        }

        // The AMF decoder stamps interlaced surfaces as field pairs, and the runtime then
        // switches h264_amf into interlaced scan mode, which stalls. Download interlaced
        // frames right after decode; the planner treats them as system frames from here.
        let (surface, filters) = if video_stream.is_interlaced() {
            (
                FrameSurface::System,
                vec![PipelineFilter::Video(VideoFilter::HwDownload(
                    HwDownloadFilter {
                        target_pixel_format: PixelFormat::Nv12,
                    },
                ))],
            )
        } else {
            (FrameSurface::Amf, Vec::new())
        };

        Some(HwDecoder {
            args: args![
                "-hwaccel",
                KnownHardwareAccel::Amf,
                "-hwaccel_output_format",
                KnownHardwareAccel::Amf
            ],
            surface,
            filters,
        })
    }

    fn accepts_upload_format(&self, pixel_format: &PixelFormat) -> bool {
        self.capabilities.vpp_supports_format(pixel_format)
    }

    fn can_convert_pixel_format(
        &self,
        _ffmpeg_info: &FfmpegInfo,
        pixel_format: &PixelFormat,
    ) -> bool {
        self.capabilities.vpp_supports_format(pixel_format)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VppAmf {
    pub(crate) size: Option<FrameSize>,
    pub(crate) format: Option<PixelFormat>,
}

impl VppAmf {
    pub(crate) fn scale(size: FrameSize) -> VppAmf {
        VppAmf {
            size: Some(size),
            ..VppAmf::default()
        }
    }

    pub(crate) fn format(format: PixelFormat) -> VppAmf {
        VppAmf {
            format: Some(format),
            ..VppAmf::default()
        }
    }

    pub(crate) fn fuse(&self, next: &VppAmf) -> Option<VppAmf> {
        if self.size.is_some() && next.size.is_some() {
            return None;
        }

        let fused = VppAmf {
            size: self.size.or(next.size),
            // later format conversion wins
            format: next.format.or(self.format),
        };

        Some(fused)
    }
}

impl VideoFilterOp for VppAmf {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.surface = FrameSurface::Amf;

        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::Amf;
            state.is_anamorphic = false;
            state.sample_aspect_ratio = Some(String::from("1:1"));
            state.display_aspect_ratio = None;
        }

        if let Some(format) = &self.format {
            state.pixel_format = *format;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Amf)
    }

    fn as_arg(&self) -> Option<String> {
        let mut options: Vec<String> = Vec::new();

        if let Some(size) = &self.size {
            options.push(format!("w={}:h={}", size.width, size.height));
        }

        if let Some(format) = &self.format {
            options.push(format!("format={}", format.as_arg()));
        }

        if options.is_empty() {
            None
        } else {
            let mut arg = format!("vpp_amf={}", options.join(":"));
            if self.size.is_some() {
                arg.push_str(",setsar=1");
            }

            Some(arg)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use libamf_sys::{AMF_SURFACE_BGRA, AMF_SURFACE_NV12, AMF_SURFACE_P010};

    use super::*;
    use crate::capabilities::amf::{AmfDevice, AmfEncoderCapability, AmfSurfaceFormat};
    use crate::frame_rate::FrameRate;
    use crate::probe::{CodecType, ProbeResultVideoStream};

    fn make_amf() -> Amf {
        make_amf_with_vpp(&[AMF_SURFACE_NV12, AMF_SURFACE_BGRA])
    }

    fn make_amf_with_vpp(vpp_formats: &[i32]) -> Amf {
        let mut supported_decoders = HashMap::new();
        supported_decoders.insert(VideoFormat::H264, vec![8]);
        supported_decoders.insert(VideoFormat::Hevc, vec![8, 10]);

        let mut supported_encoders = HashMap::new();
        supported_encoders.insert(
            VideoFormat::H264,
            AmfEncoderCapability {
                bit_depths: vec![8],
                b_frames: false,
                max_profile: Some(100),
                max_level: Some(52),
            },
        );
        supported_encoders.insert(
            VideoFormat::Hevc,
            AmfEncoderCapability {
                bit_depths: vec![8],
                b_frames: false,
                max_profile: Some(2),
                max_level: Some(186),
            },
        );

        let vpp: HashSet<AmfSurfaceFormat> =
            vpp_formats.iter().copied().map(AmfSurfaceFormat).collect();

        Amf {
            capabilities: AmfCapabilities {
                supported_decoders,
                supported_encoders,
                vpp_input_formats: vpp.clone(),
                vpp_output_formats: vpp,
                runtime_version: Some((1, 4, 31, 0)),
                device: Some(AmfDevice::Dx11),
            },
        }
    }

    fn video_stream(codec: &str, pix_fmt: &str) -> ProbeResultVideoStream {
        ProbeResultVideoStream {
            stream_index: 0,
            codec: String::from(codec),
            codec_type: CodecType::Video,
            dv_profile: None,
            profile: String::from("main"),
            height: Some(1080),
            width: Some(1920),
            frame_rate: FrameRate::parse("30000/1001"),
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            pix_fmt: String::from(pix_fmt),
            color_params: Default::default(),
            field_order: None,
        }
    }

    #[test]
    fn decode_follows_capabilities() {
        let amf = make_amf_with_vpp(&[AMF_SURFACE_NV12, AMF_SURFACE_P010, AMF_SURFACE_BGRA]);
        assert!(amf.can_decode("h264", "high", &PixelFormat::Yuv420p));
        assert!(!amf.can_decode("h264", "high 10", &PixelFormat::Yuv420p10le));
        assert!(amf.can_decode("hevc", "main 10", &PixelFormat::Yuv420p10le));
        assert!(!amf.can_decode("vp9", "profile 0", &PixelFormat::Yuv420p));
        assert!(!amf.can_decode("mpeg2video", "main", &PixelFormat::Yuv420p));
    }

    #[test]
    fn decode_requires_converter_input_support() {
        // runtime 1.4.31 decodes hevc 10-bit to P010 but its converter has no P010 input
        let amf = make_amf();
        assert!(amf.can_decode("hevc", "main", &PixelFormat::Yuv420p));
        assert!(!amf.can_decode("hevc", "main 10", &PixelFormat::Yuv420p10le));
        assert!(
            amf.make_decoder(&FfmpegInfo::default(), &video_stream("hevc", "yuv420p10le"))
                .is_none()
        );
    }

    #[test]
    fn encoder_is_gated_by_capabilities() {
        let amf = make_amf();
        assert!(amf.can_encode(&VideoFormat::Hevc, 8));
        assert!(!amf.can_encode(&VideoFormat::Hevc, 10));
        assert_eq!(
            amf.codec_for_format(&VideoFormat::Hevc, 8, None)
                .map(|c| c.codec_name),
            Some("hevc_amf")
        );
        assert!(amf.codec_for_format(&VideoFormat::Hevc, 10, None).is_none());
        assert!(amf.codec_for_format(&VideoFormat::Av1, 8, None).is_none());
    }

    #[test]
    fn upload_follows_converter_formats() {
        let amf = make_amf();
        assert!(amf.accepts_upload_format(&PixelFormat::Nv12));
        assert!(amf.accepts_upload_format(&PixelFormat::Yuv420p));
        assert!(amf.accepts_upload_format(&PixelFormat::Bgra));
        assert!(!amf.accepts_upload_format(&PixelFormat::P010le));
    }

    #[test]
    fn format_conversion_follows_converter_formats() {
        let amf = make_amf();
        let ffmpeg_info = FfmpegInfo::default();
        assert!(amf.can_convert_pixel_format(&ffmpeg_info, &PixelFormat::Nv12));
        assert!(amf.can_convert_pixel_format(&ffmpeg_info, &PixelFormat::Yuv420p));
        assert!(amf.can_convert_pixel_format(&ffmpeg_info, &PixelFormat::Bgra));
        assert!(!amf.can_convert_pixel_format(&ffmpeg_info, &PixelFormat::P010le));
        assert!(!amf.can_convert_pixel_format(&ffmpeg_info, &PixelFormat::Yuv420p10le));

        let with_p010 = make_amf_with_vpp(&[AMF_SURFACE_NV12, AMF_SURFACE_P010]);
        assert!(with_p010.can_convert_pixel_format(&ffmpeg_info, &PixelFormat::P010le));
        assert!(!with_p010.can_convert_pixel_format(&ffmpeg_info, &PixelFormat::Bgra));
    }
}
