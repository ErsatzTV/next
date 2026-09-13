use serde::Serialize;

use crate::ArgVec;
use crate::capabilities::amf::AmfCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel};
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::pipeline::{FrameSurface, PixelFormat, SurfaceSet, VideoFormat};
use crate::probe::ProbeResultVideoStream;
use crate::video_codec::VideoCodec;

#[derive(Debug, Clone, Serialize)]
pub struct Amf {
    pub capabilities: AmfCapabilities,
}

/// ffmpeg's AMF decoders are full codecs rather than hwaccels, so the decoder has
/// to be named explicitly alongside the hwaccel that keeps frames on the device.
fn decoder_name(format: &VideoFormat) -> Option<&'static str> {
    match format {
        VideoFormat::Av1 => Some("av1_amf"),
        VideoFormat::H264 => Some("h264_amf"),
        VideoFormat::Hevc => Some("hevc_amf"),
        VideoFormat::Vp9 => Some("vp9_amf"),
        _ => None,
    }
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
    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        video_format(codec).is_some_and(|f| {
            decoder_name(&f).is_some() && self.capabilities.can_decode(&f, pixel_format.bit_depth())
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

        let decoder = decoder_name(&video_format(&video_stream.codec)?)?;
        Some(HwDecoder {
            args: args![
                "-hwaccel",
                KnownHardwareAccel::Amf,
                "-hwaccel_output_format",
                KnownHardwareAccel::Amf,
                "-c:v",
                decoder,
            ],
            surface: FrameSurface::Amf,
            filters: Vec::new(),
        })
    }

    fn accepts_upload_format(&self, pixel_format: &PixelFormat) -> bool {
        self.capabilities.vpp_supports_format(pixel_format)
    }

    /// No AMF format filter is wired up yet, so conversions go through software
    fn can_convert_pixel_format(
        &self,
        _ffmpeg_info: &FfmpegInfo,
        _pixel_format: &PixelFormat,
    ) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use libamf_sys::{AMF_SURFACE_BGRA, AMF_SURFACE_NV12};

    use super::*;
    use crate::capabilities::amf::{AmfDevice, AmfEncoderCapability, AmfSurfaceFormat};
    use crate::frame_rate::FrameRate;
    use crate::probe::{CodecType, ProbeResultVideoStream};

    fn make_amf() -> Amf {
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

        let vpp: HashSet<AmfSurfaceFormat> = [AMF_SURFACE_NV12, AMF_SURFACE_BGRA]
            .into_iter()
            .map(AmfSurfaceFormat)
            .collect();

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
        let amf = make_amf();
        assert!(amf.can_decode("h264", "high", &PixelFormat::Yuv420p));
        assert!(!amf.can_decode("h264", "high 10", &PixelFormat::Yuv420p10le));
        assert!(amf.can_decode("hevc", "main 10", &PixelFormat::Yuv420p10le));
        assert!(!amf.can_decode("vp9", "profile 0", &PixelFormat::Yuv420p));
        assert!(!amf.can_decode("mpeg2video", "main", &PixelFormat::Yuv420p));
    }

    #[test]
    fn decoder_names_the_amf_codec() {
        let amf = make_amf();
        let decoder = amf
            .make_decoder(&FfmpegInfo::default(), &video_stream("hevc", "yuv420p10le"))
            .expect("hevc 10-bit decoder");
        assert_eq!(decoder.surface, FrameSurface::Amf);
        assert_eq!(
            decoder.args,
            args![
                "-hwaccel",
                "amf",
                "-hwaccel_output_format",
                "amf",
                "-c:v",
                "hevc_amf"
            ]
        );

        assert!(
            amf.make_decoder(&FfmpegInfo::default(), &video_stream("h264", "yuv420p10le"))
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
        assert!(!amf.can_convert_pixel_format(&FfmpegInfo::default(), &PixelFormat::Nv12));
    }
}
