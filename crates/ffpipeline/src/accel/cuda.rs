use crate::ArgVec;
use crate::capabilities::nvidia::NvidiaCapabilities;
use crate::ffmpeg_info::{FfmpegInfo, KnownHardwareAccel, KnownVideoFilter};
use crate::filter_chain::PipelineFilter;
use crate::frame_size::FrameSize;
use crate::hw_accel::{HwAccel, HwDecoder};
use crate::output_settings::{BwdifCudaOptions, VideoFilterOptions, YadifCudaOptions};
use crate::overlay_filter::{FramePoint, OverlayFilter, OverlayKind, OverlayKindOp};
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, SurfaceSet, VideoFormat};
use crate::probe::ProbeResultVideoStream;
use crate::video_codec::VideoCodec;
use crate::video_filter::{
    DeinterlaceFilter, ForceOriginalAspectRatio, PadFilter, ScaleFilter, ToneMapFilter,
    VideoFilter, VideoFilterOp,
};

#[derive(Debug, Clone)]
pub struct Cuda {
    pub capabilities: NvidiaCapabilities,
}

impl Cuda {
    pub fn new(capabilities: NvidiaCapabilities) -> Cuda {
        Cuda { capabilities }
    }
}

impl HwAccel for Cuda {
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
            }) if ffmpeg_info.has_video_filter(&KnownVideoFilter::ScaleCuda)
                && !current_state.pixel_format.has_alpha() =>
            {
                ScaleCuda {
                    size: *size,
                    input_is_anamorphic: *input_is_anamorphic,
                    force_original_aspect_ratio: force_original_aspect_ratio.clone(),
                }
                .into()
            }
            VideoFilter::Pad(PadFilter { size, .. })
                if ffmpeg_info.has_video_filter(&KnownVideoFilter::PadCuda) =>
            {
                PadCuda { size: *size }.into()
            }
            VideoFilter::ToneMap(ToneMapFilter {
                output_format: format,
                ..
            }) if current_state.is_hdr && current_state.surface == FrameSurface::Vulkan => {
                LibplaceboCuda {
                    algorithm: filter_options.libplacebo.tonemapping.clone(),
                    format: match format {
                        PixelFormat::Yuv420p10le => PixelFormat::P010le,
                        _ => PixelFormat::Nv12,
                    },
                }
                .into()
            }
            VideoFilter::Deinterlace(DeinterlaceFilter { .. }) => {
                let best_cuda_filter = ffmpeg_info
                    .find_best_fit(&[KnownVideoFilter::YadifCuda, KnownVideoFilter::BwdifCuda])
                    .and_then(|known_filter| {
                        CudaDeinterlaceFilter::load(known_filter, filter_options).ok()
                    });

                if let Some(best_cuda_filter) = best_cuda_filter {
                    return DeinterlaceCuda {
                        filter: best_cuda_filter,
                    }
                    .into();
                }

                video_filter.clone()
            }
            _ => video_filter.clone(),
        }
    }

    fn best_overlay(
        &self,
        overlay_filter: &OverlayFilter,
        ffmpeg_info: &FfmpegInfo,
        current_state: &FrameState,
    ) -> OverlayFilter {
        match overlay_filter.kind {
            // overlay_cuda only supports 8-bit content
            OverlayKind::Software(_)
                if ffmpeg_info.has_video_filter(&KnownVideoFilter::OverlayCuda)
                    && current_state.pixel_format.bit_depth() == 8 =>
            {
                overlay_filter.with_kind(OverlayKind::Cuda(CudaOverlay))
            }
            _ => overlay_filter.clone(),
        }
    }

    fn can_decode(&self, codec: &str, _profile: &str, pixel_format: &PixelFormat) -> bool {
        let format = match codec {
            "av1" => Some(VideoFormat::Av1),
            "h264" => Some(VideoFormat::H264),
            "hevc" => Some(VideoFormat::Hevc),
            "mpeg2video" => Some(VideoFormat::Mpeg2Video),
            "vc1" => Some(VideoFormat::Vc1),
            "vp8" => Some(VideoFormat::Vp8),
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
            VideoFormat::H264 => Some(VideoCodec {
                codec_name: "h264_nvenc",
                options: &[],
                preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                preferred_surface: FrameSurface::Cuda,
            }),
            VideoFormat::Hevc => {
                let options = if self.capabilities.b_frame_ref_mode(format) {
                    &["-tag:v", "hvc1", "-b_ref_mode", "1"]
                } else {
                    &["-tag:v", "hvc1", "-b_ref_mode", "0"]
                };

                Some(VideoCodec {
                    codec_name: "hevc_nvenc",
                    options,
                    preferred_pixel_format_8bit: Some(PixelFormat::Nv12),
                    preferred_pixel_format_10bit: Some(PixelFormat::P010le),
                    preferred_surface: FrameSurface::Cuda,
                })
            }
            _ => None,
        }
    }

    fn format_filter(&self, pixel_format: &PixelFormat) -> Option<VideoFilter> {
        Some(
            FormatCuda {
                format: *pixel_format,
            }
            .into(),
        )
    }

    fn init_hw_device(&self, surfaces: &SurfaceSet) -> ArgVec {
        if surfaces.contains(&FrameSurface::Vulkan) {
            args![
                "-init_hw_device",
                "cuda=nv",
                "-init_hw_device",
                "vulkan=vk@nv"
            ]
        } else {
            args!["-init_hw_device", "cuda"]
        }
    }

    fn known_accel(&self) -> Option<&KnownHardwareAccel> {
        Some(&KnownHardwareAccel::Cuda)
    }

    fn make_decoder(
        &self,
        ffmpeg_info: &FfmpegInfo,
        video_stream: &ProbeResultVideoStream,
    ) -> Option<HwDecoder> {
        if self.can_decode(
            &video_stream.codec,
            &video_stream.profile,
            &PixelFormat::parse(&video_stream.pix_fmt),
        ) {
            let is_vulkan_hdr = video_stream.color_params.is_hdr()
                && ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Vulkan)
                && ffmpeg_info.has_video_filter(&KnownVideoFilter::LibPlacebo);

            let decoder = if is_vulkan_hdr {
                HwDecoder {
                    args: args![
                        "-hwaccel",
                        KnownHardwareAccel::Vulkan,
                        "-hwaccel_output_format",
                        KnownHardwareAccel::Vulkan,
                    ],
                    surface: FrameSurface::Vulkan,
                    // can't work around fallback to software decode with HDR
                    filters: Vec::new(),
                }
            } else {
                HwDecoder {
                    args: args![
                        "-hwaccel",
                        KnownHardwareAccel::Cuda,
                        "-hwaccel_output_format",
                        KnownHardwareAccel::Cuda,
                    ],
                    surface: FrameSurface::Cuda,
                    filters: vec![PipelineFilter::Video(HwUploadCudaWorkaround.into())],
                }
            };

            Some(decoder)
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct ScaleCuda {
    pub(crate) size: Option<FrameSize>,
    pub(crate) input_is_anamorphic: bool,
    pub(crate) force_original_aspect_ratio: Option<ForceOriginalAspectRatio>,
}

impl VideoFilterOp for ScaleCuda {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::Cuda;
            state.is_anamorphic = false;
            state.sample_aspect_ratio = Some(String::from("1:1"));
            state.display_aspect_ratio = None;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Cuda)
    }

    fn as_arg(&self) -> Option<String> {
        if let Some(size) = &self.size {
            let aspect_ratio = self
                .force_original_aspect_ratio
                .as_ref()
                .map_or(String::new(), |f| f.as_arg());

            if self.input_is_anamorphic {
                Some(format!(
                    "scale_cuda=iw*sar:ih,scale_cuda={}:{}{},setsar=1",
                    size.width, size.height, aspect_ratio
                ))
            } else {
                Some(format!(
                    "scale_cuda={}:{}{},setsar=1",
                    size.width, size.height, aspect_ratio
                ))
            }
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct PadCuda {
    pub(crate) size: Option<FrameSize>,
}

impl VideoFilterOp for PadCuda {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::Cuda;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Cuda)
    }

    fn as_arg(&self) -> Option<String> {
        self.size.as_ref().map(|s| {
            format!(
                "pad_cuda={}:{}:-1:-1:color=black,setsar=1",
                s.width, s.height
            )
        })
    }
}

#[derive(Clone)]
pub struct FormatCuda {
    pub(crate) format: PixelFormat,
}

impl VideoFilterOp for FormatCuda {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format;
        state.surface = FrameSurface::Cuda;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Cuda)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!("scale_cuda=format={}", self.format.as_arg()))
    }
}

#[derive(Clone)]
pub struct HwUploadCudaWorkaround;

impl VideoFilterOp for HwUploadCudaWorkaround {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        // we always need to keep this filter
        Some(self.clone().into())
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.surface = FrameSurface::Cuda;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        // saying cuda because we don't want the pipeline to download before uploading
        Some(FrameSurface::Cuda)
    }

    fn as_arg(&self) -> Option<String> {
        Some(String::from("hwupload"))
    }
}

#[derive(Clone)]
pub struct LibplaceboCuda {
    /// algorithm to use for tonemapping
    pub(crate) algorithm: Option<String>,
    pub(crate) format: PixelFormat,
}

impl VideoFilterOp for LibplaceboCuda {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format;
        state.is_hdr = false;
        state.surface = FrameSurface::Cuda;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Vulkan)
    }

    fn as_arg(&self) -> Option<String> {
        let vulkan_format = match &self.format {
            PixelFormat::P010le => PixelFormat::P016,
            _ => PixelFormat::Nv12,
        };

        let cuda_format = match vulkan_format {
            PixelFormat::P016 => ",scale_cuda=format=p010",
            _ => "",
        };

        Some(format!(
            "libplacebo=tonemapping={}:colorspace=bt709:color_primaries=bt709:color_trc=bt709:format={},hwupload_cuda{}",
            self.algorithm.as_deref().unwrap_or("linear"),
            vulkan_format.as_arg(),
            cuda_format
        ))
    }
}

#[derive(Clone)]
pub struct DeinterlaceCuda {
    pub(crate) filter: CudaDeinterlaceFilter,
}

#[derive(Clone)]
pub enum CudaDeinterlaceFilter {
    Bwdif(BwdifCudaOptions),
    Yadif(YadifCudaOptions),
}

impl CudaDeinterlaceFilter {
    fn load(
        known_filter: &KnownVideoFilter,
        filter_options: &VideoFilterOptions,
    ) -> Result<CudaDeinterlaceFilter, String> {
        match known_filter {
            KnownVideoFilter::BwdifCuda => Ok(CudaDeinterlaceFilter::Bwdif(
                filter_options.bwdif_cuda.clone(),
            )),
            KnownVideoFilter::YadifCuda => Ok(CudaDeinterlaceFilter::Yadif(
                filter_options.yadif_cuda.clone(),
            )),
            _ => Err(format!("Unknown cuda deinterlace filter: {}", known_filter)),
        }
    }
}

impl VideoFilterOp for DeinterlaceCuda {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.is_interlaced = false;
        state.surface = FrameSurface::Cuda;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::Cuda)
    }

    fn as_arg(&self) -> Option<String> {
        match &self.filter {
            CudaDeinterlaceFilter::Bwdif(options) => {
                let mode = options.mode.as_deref().unwrap_or("1");
                Some(format!("bwdif_cuda={mode}"))
            }
            CudaDeinterlaceFilter::Yadif(options) => {
                let mode = options.mode.as_deref().unwrap_or("1");
                Some(format!("yadif_cuda={mode}"))
            }
        }
    }
}

#[derive(Clone)]
pub struct CudaOverlay;

impl OverlayKindOp for CudaOverlay {
    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = PixelFormat::Nv12;
        state.surface = FrameSurface::Cuda;
    }

    fn main_input_state(&self, current_state: &FrameState) -> FrameState {
        FrameState {
            pixel_format: PixelFormat::Yuv420p,
            surface: FrameSurface::Cuda,
            ..current_state.clone()
        }
    }

    fn secondary_input_state(&self, current_state: &FrameState) -> FrameState {
        FrameState {
            pixel_format: PixelFormat::Yuva420p,
            surface: FrameSurface::Cuda,
            ..current_state.clone()
        }
    }

    fn as_arg(&self, location: Option<FramePoint>) -> Option<String> {
        if let Some(location) = location {
            Some(format!("overlay_cuda=x={}:y={}", location.x, location.y))
        } else {
            Some(String::from("overlay_cuda=x=(W-w)/2:y=(H-h)/2"))
        }
    }
}
