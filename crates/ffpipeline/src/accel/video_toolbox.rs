use serde::Serialize;

use crate::ArgVec;
use crate::capabilities::videotoolbox::VideoToolboxCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::output_settings::VideoFilterOptions;
use crate::pipeline::{
    EncodeFormat, FrameState, FrameSurface, PixelFormat, SurfaceSet, VideoFormat,
};
use crate::probe::ProbeResultVideoStream;
use crate::video_codec::{MetadataBsf, VideoCodec};
use crate::video_filter::{ScaleFilter, VideoFilter, VideoFilterOp};

#[derive(Debug, Clone, Serialize)]
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
        _filter_options: &VideoFilterOptions,
    ) -> VideoFilter {
        match video_filter {
            VideoFilter::Scale(ScaleFilter { size, .. })
                if ffmpeg_info.has_video_filter(&KnownVideoFilter::ScaleVt) =>
            {
                ScaleVt { size: *size }.into()
            }
            _ => video_filter.clone(),
        }
    }

    fn can_convert_pixel_format(
        &self,
        _ffmpeg_info: &FfmpegInfo,
        _from: &PixelFormat,
        to: &PixelFormat,
    ) -> bool {
        // TODO: clean this up when we can model things more accurately
        to.bit_depth() == 8
    }

    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        codec
            .parse::<VideoFormat>()
            .is_ok_and(|f| self.capabilities.can_decode(&f, pixel_format.bit_depth()))
    }

    // apple silicon has no hardware decode for interlaced h264
    fn can_decode_stream(&self, video_stream: &ProbeResultVideoStream) -> bool {
        let Ok(format) = video_stream.codec.parse::<VideoFormat>() else {
            return false;
        };
        let bit_depth = PixelFormat::parse(&video_stream.pix_fmt).bit_depth();
        if video_stream.is_interlaced() && format.has_interlaced_coding() {
            self.capabilities.can_decode_interlaced(&format, bit_depth)
        } else {
            self.capabilities.can_decode(&format, bit_depth)
        }
    }

    fn can_encode(&self, format: &EncodeFormat, bit_depth: u8) -> bool {
        self.capabilities
            .can_encode(&VideoFormat::from(*format), bit_depth)
    }

    fn codec_for_format(
        &self,
        format: &EncodeFormat,
        bit_depth: u8,
        _video_size: Option<FrameSize>,
    ) -> Option<VideoCodec> {
        match format {
            EncodeFormat::H264 if self.can_encode(format, 8) => Some(VideoCodec {
                codec_name: "h264_videotoolbox",
                // work around an ffmpeg bug where the encoder fails to parse its own SEI
                // when adding captions
                options: args!["-a53cc", "0"],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::VideoToolbox,
                also_accepts: &[],
            }),
            EncodeFormat::Hevc if self.can_encode(format, 8) => Some(VideoCodec {
                codec_name: "hevc_videotoolbox",
                options: match bit_depth {
                    10 => args!["-profile:v", "main10"],
                    8 => args!["-profile:v", "main"],
                    _ => Vec::new(),
                },
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::VideoToolbox,
                also_accepts: &[],
            }),
            _ => None,
        }
    }

    // m1 mac mini was observed not to tag SAR with h264 encoder
    fn metadata_bsf(&self, codec: &VideoCodec) -> Option<MetadataBsf> {
        (codec.codec_name == "h264_videotoolbox").then_some(MetadataBsf {
            filter: "h264_metadata",
            square_pixels: true,
            bt709: false,
        })
    }

    fn init_hw_device(&self, _surfaces: &SurfaceSet) -> ArgVec {
        args!["-init_hw_device", "videotoolbox"]
    }

    fn known_accel(&self) -> Option<&KnownHardwareAccel> {
        Some(&KnownHardwareAccel::VideoToolbox)
    }

    fn make_decoder(
        &self,
        _ffmpeg_info: &FfmpegInfo,
        video_stream: &ProbeResultVideoStream,
    ) -> Option<HwDecoder> {
        if !self.can_decode_stream(video_stream) {
            return None;
        }

        Some(HwDecoder {
            args: args![
                "-hwaccel",
                "videotoolbox",
                "-hwaccel_output_format",
                "videotoolbox_vld",
            ],
            surface: FrameSurface::VideoToolbox,
            filters: Vec::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ScaleVt {
    pub(crate) size: Option<FrameSize>,
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
        self.size
            .as_ref()
            .map(|size| format!("scale_vt={}:{},setsar=1", size.width, size.height))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::frame_rate::FrameRate;
    use crate::probe::CodecType;

    fn video_toolbox() -> VideoToolbox {
        VideoToolbox::new(VideoToolboxCapabilities {
            supported_decoders: HashSet::from([(VideoFormat::H264, 8), (VideoFormat::Hevc, 8)]),
            supported_interlaced_decoders: HashSet::new(),
            supported_encoders: HashSet::new(),
        })
    }

    fn video_stream(codec: &str, field_order: &str) -> ProbeResultVideoStream {
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
            pix_fmt: String::from("yuv420p"),
            color_params: Default::default(),
            field_order: Some(String::from(field_order)),
            rotation: None,
        }
    }

    #[test]
    fn interlaced_h264_requires_interlaced_decoder() {
        let vt = video_toolbox();
        assert!(vt.can_decode_stream(&video_stream("h264", "progressive")));
        assert!(!vt.can_decode_stream(&video_stream("h264", "tt")));
        assert!(
            vt.make_decoder(&FfmpegInfo::default(), &video_stream("h264", "tt"))
                .is_none()
        );
    }

    #[test]
    fn interlaced_flag_ignored_without_interlaced_coding() {
        // field order can come from the container; hevc fields decode as progressive pictures
        let vt = video_toolbox();
        assert!(vt.can_decode_stream(&video_stream("hevc", "tt")));
    }
}
