use serde::Serialize;

use crate::ArgVec;
use crate::capabilities::amf::AmfCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownDecoders, KnownHardwareAccel, KnownVideoFilter};
use crate::filter_chain::PipelineFilter;
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::output_settings::VideoFilterOptions;
use crate::pipeline::{
    FrameState, FrameSurface, HdrFormat, HwPixelFormat, PixelFormat, SurfaceSet, VideoFormat,
};
use crate::probe::ProbeResultVideoStream;
use crate::video_codec::VideoCodec;
use crate::video_filter::{
    HwDownloadFilter, ScaleFilter, ToneMapFilter, VideoFilter, VideoFilterOp,
};

#[derive(Debug, Clone, Serialize)]
pub struct Amf {
    pub capabilities: AmfCapabilities,
}

fn video_format(codec: &str) -> Option<VideoFormat> {
    match codec {
        "av1" => Some(VideoFormat::Av1),
        "h264" => Some(VideoFormat::H264),
        "hevc" => Some(VideoFormat::Hevc),
        "mpeg2video" => Some(VideoFormat::Mpeg2Video),
        "vc1" => Some(VideoFormat::Vc1),
        "vp9" => Some(VideoFormat::Vp9),
        _ => None,
    }
}

/// `-hwaccel amf` makes ffmpeg use this decoder, so the build must include it. Stock
/// ffmpeg ships av1, h264, hevc and vp9; mpeg2_amf comes from the etv patch.
/// If the decoder is missing, ffmpeg falls back to software decoding with no warning.
///
/// vc1_amf is in the same patch but is left out on purpose. The AMF runtime gives each
/// output picture the pts of its own packet. VC-1 in MKV has decode-order timestamps,
/// so output pts swap in pairs, and a cfr encode then duplicates and drops about
/// a third of the frames. The native decoder avoids this with a one-frame reorder.
fn amf_decoder(codec: &str) -> Option<&'static str> {
    match codec {
        "av1" => Some(KnownDecoders::Av1Amf.into()),
        "h264" => Some(KnownDecoders::H264Amf.into()),
        "hevc" => Some(KnownDecoders::HevcAmf.into()),
        "mpeg2video" => Some(KnownDecoders::Mpeg2Amf.into()),
        "vp9" => Some(KnownDecoders::Vp9Amf.into()),
        _ => None,
    }
}

impl Amf {
    /// PQ sources bypass [`Amf::can_decode`]: the converter lists no P010 input, but
    /// runtimes that can tonemap accept decoder PQ surfaces.
    fn can_decode_for_tonemap(
        &self,
        video_stream: &ProbeResultVideoStream,
        pixel_format: &PixelFormat,
    ) -> bool {
        video_stream.color_params.is_pq()
            && self.capabilities.can_tonemap()
            && video_format(&video_stream.codec)
                .is_some_and(|f| self.capabilities.can_decode(&f, pixel_format.bit_depth()))
    }
}

impl HwAccel for Amf {
    fn best_filter(
        &self,
        video_filter: &VideoFilter,
        ffmpeg_info: &FfmpegInfo,
        current_state: &FrameState,
        _filter_options: &VideoFilterOptions,
    ) -> VideoFilter {
        match video_filter {
            VideoFilter::Scale(ScaleFilter {
                size: Some(size), ..
            }) if ffmpeg_info.has_video_filter(&KnownVideoFilter::VppAmf) => {
                VppAmf::scale(*size).into()
            }
            // The converter reads the transfer characteristics the decoder stamped on the
            // surface, so only surfaces straight from the AMF decoder can be tone mapped.
            // P010 output skips the gamut mapping (washed out grey), so 10-bit SDR output
            // stays on the software tonemap.
            VideoFilter::ToneMap(ToneMapFilter {
                output_format: format,
                ..
            }) if ffmpeg_info.has_video_filter(&KnownVideoFilter::VppAmf)
                && self.capabilities.can_tonemap()
                && current_state.surface == FrameSurface::Amf
                && matches!(current_state.hdr_format, HdrFormat::Hdr10 | HdrFormat::Pq)
                && self.output_format(format) == HwPixelFormat::Nv12 =>
            {
                VppAmf::tonemap().into()
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

    /// ffmpeg's AMF context ignores its device string. The only way to pick an adapter
    /// is to derive AMF from a `d3d11va` device, which does take an adapter index.
    fn init_hw_device(&self, _surfaces: &SurfaceSet) -> ArgVec {
        match self.capabilities.adapter() {
            Some(adapter) => args![
                "-init_hw_device",
                format!("d3d11va=dx:{}", adapter.index),
                "-init_hw_device",
                "amf=hw@dx",
                "-filter_hw_device",
                "hw"
            ],
            None => args!["-init_hw_device", "amf=hw", "-filter_hw_device", "hw"],
        }
    }

    fn known_accel(&self) -> Option<&KnownHardwareAccel> {
        Some(&KnownHardwareAccel::Amf)
    }

    fn make_decoder(
        &self,
        ffmpeg_info: &FfmpegInfo,
        video_stream: &ProbeResultVideoStream,
    ) -> Option<HwDecoder> {
        if !amf_decoder(&video_stream.codec).is_some_and(|d| ffmpeg_info.has_decoder(d)) {
            return None;
        }

        let pixel_format = PixelFormat::parse(&video_stream.pix_fmt);
        if !self.can_decode(&video_stream.codec, &video_stream.profile, &pixel_format)
            && !self.can_decode_for_tonemap(video_stream, &pixel_format)
        {
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
    pub(crate) tonemap: bool,
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

    /// PQ to bt709 conversion. Only nv12 output maps the gamut correctly.
    pub(crate) fn tonemap() -> VppAmf {
        VppAmf {
            tonemap: true,
            format: Some(PixelFormat::Nv12),
            ..VppAmf::default()
        }
    }

    pub(crate) fn fuse(&self, next: &VppAmf) -> Option<VppAmf> {
        if self.size.is_some() && next.size.is_some() {
            return None;
        }

        let fused = VppAmf {
            tonemap: self.tonemap || next.tonemap,
            size: self.size.or(next.size),
            // later format conversion wins
            format: next.format.or(self.format),
        };

        // the converter only tone maps correctly to nv12
        if fused.tonemap && fused.format != Some(PixelFormat::Nv12) {
            return None;
        }

        Some(fused)
    }
}

impl VideoFilterOp for VppAmf {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.surface = FrameSurface::Amf;

        if self.tonemap {
            state.hdr_format = HdrFormat::None;
        }

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

        if self.tonemap {
            options.push(String::from(
                "color_profile=bt709:primaries=bt709:trc=bt709",
            ));
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
    use crate::capabilities::amf::{AmfAdapter, AmfDevice, AmfEncoderCapability, AmfSurfaceFormat};
    use crate::frame_rate::FrameRate;
    use crate::probe::{CodecType, ProbeResultColorParams, ProbeResultVideoStream};

    /// Models a build with the etv patch, so mpeg2_amf and vc1_amf exist.
    fn make_ffmpeg_info() -> FfmpegInfo {
        FfmpegInfo {
            decoders: [
                "h264_amf",
                "hevc_amf",
                "vp9_amf",
                "av1_amf",
                "mpeg2_amf",
                "vc1_amf",
            ]
            .map(String::from)
            .into(),
            video_filters: HashSet::from([KnownVideoFilter::VppAmf.to_string()]),
            ..Default::default()
        }
    }

    fn hdr_amf_state() -> FrameState {
        FrameState {
            size: FrameSize {
                width: 3840,
                height: 2160,
            },
            is_anamorphic: false,
            is_interlaced: false,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            surface: FrameSurface::Amf,
            pixel_format: PixelFormat::P010le,
            hdr_format: HdrFormat::Hdr10,
            rotation: None,
        }
    }

    fn tonemap_filter(output_format: PixelFormat) -> VideoFilter {
        ToneMapFilter {
            algorithm: Some(String::from("hable")),
            output_format,
        }
        .into()
    }

    fn make_amf() -> Amf {
        make_amf_with_vpp(&[AMF_SURFACE_NV12, AMF_SURFACE_BGRA])
    }

    fn make_amf_with_vpp(vpp_formats: &[i32]) -> Amf {
        make_amf_with_runtime(Some((1, 4, 37, 0)), vpp_formats)
    }

    fn make_amf_with_runtime(
        runtime_version: Option<(u16, u16, u16, u16)>,
        vpp_formats: &[i32],
    ) -> Amf {
        let mut supported_decoders = HashMap::new();
        supported_decoders.insert(VideoFormat::H264, vec![8]);
        supported_decoders.insert(VideoFormat::Hevc, vec![8, 10]);
        supported_decoders.insert(VideoFormat::Mpeg2Video, vec![8]);
        supported_decoders.insert(VideoFormat::Vc1, vec![8]);

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
                runtime_version,
                device: Some(AmfDevice::Dx11),
                adapter: None,
            },
        }
    }

    #[test]
    fn init_hw_device_derives_from_d3d11va_on_chosen_adapter() {
        let surfaces = SurfaceSet::default();
        assert_eq!(
            make_amf().init_hw_device(&surfaces),
            vec!["-init_hw_device", "amf=hw", "-filter_hw_device", "hw"]
        );

        let mut amf = make_amf();
        amf.capabilities.adapter = Some(AmfAdapter {
            index: 1,
            description: String::from("AMD Radeon RX 6600M"),
            vendor_id: 0x1002,
            device_id: 0x73ff,
        });
        assert_eq!(
            amf.init_hw_device(&surfaces),
            vec![
                "-init_hw_device",
                "d3d11va=dx:1",
                "-init_hw_device",
                "amf=hw@dx",
                "-filter_hw_device",
                "hw"
            ]
        );
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
            rotation: None,
        }
    }

    fn pq_video_stream() -> ProbeResultVideoStream {
        ProbeResultVideoStream {
            profile: String::from("main 10"),
            color_params: ProbeResultColorParams {
                color_range: Some(String::from("tv")),
                color_space: Some(String::from("bt2020nc")),
                color_transfer: Some(String::from("smpte2084")),
                color_primaries: Some(String::from("bt2020")),
                has_hdr10_metadata: true,
            },
            ..video_stream("hevc", "yuv420p10le")
        }
    }

    #[test]
    fn decode_follows_capabilities() {
        let amf = make_amf_with_vpp(&[AMF_SURFACE_NV12, AMF_SURFACE_P010, AMF_SURFACE_BGRA]);
        assert!(amf.can_decode("h264", "high", &PixelFormat::Yuv420p));
        assert!(!amf.can_decode("h264", "high 10", &PixelFormat::Yuv420p10le));
        assert!(amf.can_decode("hevc", "main 10", &PixelFormat::Yuv420p10le));
        assert!(!amf.can_decode("vp9", "profile 0", &PixelFormat::Yuv420p));
        assert!(amf.can_decode("mpeg2video", "main", &PixelFormat::Yuv420p));
        assert!(amf.can_decode("vc1", "advanced", &PixelFormat::Yuv420p));
        assert!(!amf.can_decode("mpeg4", "simple", &PixelFormat::Yuv420p));
    }

    #[test]
    fn decoder_requires_the_amf_decoder_in_ffmpeg() {
        let amf = make_amf_with_vpp(&[AMF_SURFACE_NV12, AMF_SURFACE_P010, AMF_SURFACE_BGRA]);
        let with_patch = make_ffmpeg_info();
        assert!(
            amf.make_decoder(&with_patch, &video_stream("mpeg2video", "yuv420p"))
                .is_some()
        );
        // hardware and build both support vc1, but vc1_amf breaks timestamps
        assert!(
            amf.make_decoder(&with_patch, &video_stream("vc1", "yuv420p"))
                .is_none()
        );

        // stock ffmpeg, without the etv patch
        let stock = FfmpegInfo {
            decoders: ["h264_amf", "hevc_amf", "vp9_amf", "av1_amf"]
                .map(String::from)
                .into(),
            ..make_ffmpeg_info()
        };
        assert!(
            amf.make_decoder(&stock, &video_stream("h264", "yuv420p"))
                .is_some()
        );
        assert!(
            amf.make_decoder(&stock, &video_stream("mpeg2video", "yuv420p"))
                .is_none()
        );
        assert!(
            amf.make_decoder(&stock, &video_stream("vc1", "yuv420p"))
                .is_none()
        );

        assert!(
            amf.make_decoder(&FfmpegInfo::default(), &video_stream("h264", "yuv420p"))
                .is_none()
        );
    }

    #[test]
    fn decode_requires_converter_input_support() {
        // runtime 1.4.31 decodes hevc 10-bit to P010 but its converter has no P010 input
        let amf = make_amf_with_runtime(Some((1, 4, 31, 0)), &[AMF_SURFACE_NV12, AMF_SURFACE_BGRA]);
        assert!(amf.can_decode("hevc", "main", &PixelFormat::Yuv420p));
        assert!(!amf.can_decode("hevc", "main 10", &PixelFormat::Yuv420p10le));
        assert!(
            amf.make_decoder(&make_ffmpeg_info(), &video_stream("hevc", "yuv420p10le"))
                .is_none()
        );
        assert!(
            amf.make_decoder(&make_ffmpeg_info(), &pq_video_stream())
                .is_none()
        );
    }

    #[test]
    fn pq_sources_decode_on_device_when_converter_can_tonemap() {
        // caps still list no P010 input; only the runtime version unlocks pq decode
        let amf = make_amf();
        assert!(!amf.can_decode("hevc", "main 10", &PixelFormat::Yuv420p10le));
        assert!(
            amf.make_decoder(&make_ffmpeg_info(), &video_stream("hevc", "yuv420p10le"))
                .is_none(),
            "sdr 10-bit must stay in software: the converter mangles P010 to NV12"
        );

        let decoder = amf
            .make_decoder(&make_ffmpeg_info(), &pq_video_stream())
            .expect("pq source should decode on the device");
        assert_eq!(decoder.surface, FrameSurface::Amf);

        let hlg = ProbeResultVideoStream {
            color_params: ProbeResultColorParams {
                color_transfer: Some(String::from("arib-std-b67")),
                ..pq_video_stream().color_params
            },
            ..pq_video_stream()
        };
        assert!(amf.make_decoder(&make_ffmpeg_info(), &hlg).is_none());
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

    #[test]
    fn best_filter_selects_vpp_amf_tonemap_for_hdr_to_8bit_sdr() {
        let amf = make_amf();
        let ffmpeg_info = make_ffmpeg_info();
        let filter_options = VideoFilterOptions::default();

        for hdr_format in [HdrFormat::Hdr10, HdrFormat::Pq] {
            let state = FrameState {
                hdr_format,
                ..hdr_amf_state()
            };
            let result = amf.best_filter(
                &tonemap_filter(PixelFormat::Yuv420p),
                &ffmpeg_info,
                &state,
                &filter_options,
            );
            assert!(
                matches!(
                    result,
                    VideoFilter::VppAmf(VppAmf {
                        tonemap: true,
                        format: Some(PixelFormat::Nv12),
                        ..
                    })
                ),
                "expected vpp_amf tonemap for {hdr_format:?}, got {result:?}"
            );
        }
    }

    #[test]
    fn best_filter_keeps_software_tonemap_when_vpp_amf_cannot_be_used() {
        let amf = make_amf();
        let ffmpeg_info = make_ffmpeg_info();
        let filter_options = VideoFilterOptions::default();

        // 10-bit sdr output: the converter's p010 output skips the gamut mapping
        let result = amf.best_filter(
            &tonemap_filter(PixelFormat::Yuv420p10le),
            &ffmpeg_info,
            &hdr_amf_state(),
            &filter_options,
        );
        assert!(matches!(result, VideoFilter::ToneMap(_)), "got {result:?}");

        // system frames carry no amf surface metadata for the converter to read
        let state = FrameState {
            surface: FrameSurface::System,
            ..hdr_amf_state()
        };
        let result = amf.best_filter(
            &tonemap_filter(PixelFormat::Yuv420p),
            &ffmpeg_info,
            &state,
            &filter_options,
        );
        assert!(matches!(result, VideoFilter::ToneMap(_)), "got {result:?}");

        // only pq input has been verified
        for hdr_format in [HdrFormat::Hlg, HdrFormat::Dv5, HdrFormat::None] {
            let state = FrameState {
                hdr_format,
                ..hdr_amf_state()
            };
            let result = amf.best_filter(
                &tonemap_filter(PixelFormat::Yuv420p),
                &ffmpeg_info,
                &state,
                &filter_options,
            );
            assert!(
                matches!(result, VideoFilter::ToneMap(_)),
                "expected software tonemap for {hdr_format:?}, got {result:?}"
            );
        }

        // no vpp_amf filter in ffmpeg
        let result = amf.best_filter(
            &tonemap_filter(PixelFormat::Yuv420p),
            &FfmpegInfo::default(),
            &hdr_amf_state(),
            &filter_options,
        );
        assert!(matches!(result, VideoFilter::ToneMap(_)), "got {result:?}");

        // runtime too old for the converter's color management
        let old_runtime =
            make_amf_with_runtime(Some((1, 4, 31, 0)), &[AMF_SURFACE_NV12, AMF_SURFACE_BGRA]);
        let result = old_runtime.best_filter(
            &tonemap_filter(PixelFormat::Yuv420p),
            &ffmpeg_info,
            &hdr_amf_state(),
            &filter_options,
        );
        assert!(matches!(result, VideoFilter::ToneMap(_)), "got {result:?}");
    }

    #[test]
    fn tonemap_fuses_with_scale_and_clears_hdr() {
        let scale = VppAmf::scale(FrameSize {
            width: 1920,
            height: 1080,
        });
        let fused = scale.fuse(&VppAmf::tonemap()).unwrap();
        assert_eq!(
            fused.as_arg().as_deref(),
            Some(
                "vpp_amf=w=1920:h=1080:format=nv12:color_profile=bt709:primaries=bt709:trc=bt709,setsar=1"
            )
        );

        let fused = VppAmf::tonemap().fuse(&scale).unwrap();
        assert_eq!(fused.size, scale.size);
        assert!(fused.tonemap);

        let mut state = hdr_amf_state();
        fused.apply_to(&mut state);
        assert_eq!(state.hdr_format, HdrFormat::None);
        assert_eq!(state.pixel_format, PixelFormat::Nv12);
        assert_eq!(state.surface, FrameSurface::Amf);
        assert_eq!(state.size.width, 1920);
    }

    #[test]
    fn tonemap_does_not_fuse_with_non_nv12_format() {
        let tonemap = VppAmf::tonemap();
        assert!(tonemap.fuse(&VppAmf::format(PixelFormat::P010le)).is_none());
        assert!(
            VppAmf::format(PixelFormat::P010le)
                .fuse(&tonemap)
                .is_some_and(|f| f.format == Some(PixelFormat::Nv12))
        );
    }
}
