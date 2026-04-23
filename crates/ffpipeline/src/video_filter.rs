use enum_dispatch::enum_dispatch;

use crate::accel;
use crate::ffmpeg_info::{FfmpegInfo, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat};

#[derive(Clone)]
pub enum ForceOriginalAspectRatio {
    Increase,
    Decrease,
}

impl ForceOriginalAspectRatio {
    pub(crate) fn as_arg(&self) -> String {
        match self {
            ForceOriginalAspectRatio::Increase => {
                String::from(":force_original_aspect_ratio=increase")
            }
            ForceOriginalAspectRatio::Decrease => {
                String::from(":force_original_aspect_ratio=decrease")
            }
        }
    }
}

#[derive(Clone)]
pub enum SoftwareDeinterlaceFilter {
    Bwdif,
    Yadif,
    W3fdif,
}

#[enum_dispatch]
pub trait VideoFilterOp {
    fn evaluate(&self, state: &FrameState, ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter>;
    fn apply_to(&self, state: &mut FrameState);
    fn required_surface(&self) -> Option<FrameSurface>;
    fn as_arg(&self) -> Option<String>;
}

#[derive(Clone)]
#[enum_dispatch(VideoFilterOp)]
pub enum VideoFilter {
    HwUpload(HwUploadFilter),
    HwDownload(HwDownloadFilter),
    Scale(ScaleFilter),
    Pad(PadFilter),
    Loop(LoopFilter),
    Format(FormatFilter),
    ToneMap(ToneMapFilter),
    Deinterlace(DeinterlaceFilter),
    HwMap(HwMapFilter),
    // CUDA hardware filters
    ScaleCuda(accel::cuda::ScaleCuda),
    PadCuda(accel::cuda::PadCuda),
    FormatCuda(accel::cuda::FormatCuda),
    HwUploadCudaWorkaround(accel::cuda::HwUploadCudaWorkaround),
    LibplaceboCuda(accel::cuda::LibplaceboCuda),
    DeinterlaceCuda(accel::cuda::DeinterlaceCuda),
    // VAAPI hardware filters
    ScaleVaapi(accel::vaapi::ScaleVaapi),
    PadVaapi(accel::vaapi::PadVaapi),
    FormatVaapi(accel::vaapi::FormatVaapi),
    // OpenCL hardware filters
    TonemapOpencl(accel::opencl::TonemapOpencl),
    // QSV hardware filters
    ScaleQsv(accel::qsv::ScaleQsv),
    FormatQsv(accel::qsv::FormatQsv),
    DeinterlaceQsv(accel::qsv::DeinterlaceQsv),
    // Vulkan hardware filters
    ScaleVulkan(accel::vulkan::ScaleVulkan),
    FormatVulkan(accel::vulkan::FormatVulkan),
    LibplaceboVulkan(accel::vulkan::LibplaceboVulkan),
}

// --- Software filter structs ---

#[derive(Clone)]
pub struct HwUploadFilter {
    pub target_surface: FrameSurface,
    pub source_format: PixelFormat,
}

impl VideoFilterOp for HwUploadFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.surface = self.target_surface;
        state.pixel_format = match &state.pixel_format {
            PixelFormat::Yuv420p => PixelFormat::Nv12,
            PixelFormat::Yuv420p10le => PixelFormat::P010le,
            other => other.clone(),
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        None
    }

    fn as_arg(&self) -> Option<String> {
        let target_format = match self.source_format.bit_depth() {
            10 => PixelFormat::P010le,
            _ => PixelFormat::Nv12,
        };

        let format_filter = if self.source_format == target_format {
            String::new()
        } else {
            format!("format={},", target_format.as_arg())
        };

        match &self.target_surface {
            FrameSurface::Cuda => Some(format!("{format_filter}hwupload_cuda")),
            FrameSurface::Qsv => Some(format!("{format_filter}hwupload=extra_hw_frames=64")),
            FrameSurface::Vaapi => Some(format!("{format_filter}hwupload")),
            FrameSurface::Vulkan => Some(format!("{format_filter}hwupload")),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct HwDownloadFilter {
    pub target_pixel_format: PixelFormat,
}

impl VideoFilterOp for HwDownloadFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.surface = FrameSurface::System;
        state.pixel_format = self.target_pixel_format.clone();
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        None
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!(
            "hwdownload,format={}",
            self.target_pixel_format.as_arg()
        ))
    }
}

#[derive(Clone)]
pub struct ScaleFilter {
    pub size: Option<FrameSize>,
    pub input_is_anamorphic: bool,
    pub force_original_aspect_ratio: Option<ForceOriginalAspectRatio>,
}

impl VideoFilterOp for ScaleFilter {
    fn evaluate(&self, state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        match &self.size {
            Some(target) => {
                if state.size == *target {
                    None
                } else {
                    let actual = target.square_pixel_size(state);
                    let force_original_aspect_ratio = if actual == *target {
                        None
                    } else {
                        Some(ForceOriginalAspectRatio::Decrease)
                    };

                    Some(
                        ScaleFilter {
                            size: Some(actual),
                            input_is_anamorphic: state.is_anamorphic,
                            force_original_aspect_ratio,
                        }
                        .into(),
                    )
                }
            }
            None => None,
        }
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.is_anamorphic = false;
            state.sample_aspect_ratio = Some(String::from("1:1"));
            state.display_aspect_ratio = None;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        if let Some(size) = &self.size {
            let aspect_ratio = self
                .force_original_aspect_ratio
                .as_ref()
                .map_or(String::new(), |f| f.as_arg());

            if self.input_is_anamorphic {
                Some(format!(
                    "scale=iw*sar:ih,setsar=1,scale={}:{}:flags=fast_bilinear{}",
                    size.width, size.height, aspect_ratio
                ))
            } else {
                Some(format!(
                    "scale={}:{}:flags=fast_bilinear{},setsar=1",
                    size.width, size.height, aspect_ratio
                ))
            }
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct PadFilter {
    pub size: Option<FrameSize>,
}

impl VideoFilterOp for PadFilter {
    fn evaluate(&self, state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        match &self.size {
            Some(target) if state.size != *target => Some(self.clone().into()),
            _ => None,
        }
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::System;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        self.size
            .as_ref()
            .map(|size| format!("pad={}:{}:-1:-1:color=black", size.width, size.height))
    }
}

#[derive(Clone)]
pub struct LoopFilter {
    pub codec: String,
}

impl VideoFilterOp for LoopFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if self.codec == "png" {
            Some(self.clone().into())
        } else {
            None
        }
    }

    fn apply_to(&self, _state: &mut FrameState) {}

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        Some(String::from("loop=-1:1"))
    }
}

#[derive(Clone)]
pub struct FormatFilter {
    pub format: PixelFormat,
}

impl VideoFilterOp for FormatFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format.clone();
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!("format={}", self.format.as_arg()))
    }
}

#[derive(Clone)]
pub struct ToneMapFilter {
    pub algorithm: Option<String>,
    pub format: PixelFormat,
}

impl VideoFilterOp for ToneMapFilter {
    fn evaluate(&self, state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if state.is_hdr {
            Some(self.clone().into())
        } else {
            None
        }
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format.clone();
        state.is_hdr = false;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!(
            "zscale=transfer=linear,tonemap={},zscale=transfer=bt709,format={}",
            self.algorithm.as_deref().unwrap_or("linear"),
            self.format.as_arg()
        ))
    }
}

#[derive(Clone)]
pub struct DeinterlaceFilter {
    pub filter: SoftwareDeinterlaceFilter,
    pub input_is_interlaced: bool,
}

impl VideoFilterOp for DeinterlaceFilter {
    fn evaluate(&self, _state: &FrameState, ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if self.input_is_interlaced {
            let best = ffmpeg_info.find_best_fit(&[
                KnownVideoFilter::Yadif,
                KnownVideoFilter::Bwdif,
                KnownVideoFilter::W3fdif,
            ]);

            if let Some(known_filter) = best {
                let software_filter = match known_filter {
                    KnownVideoFilter::Yadif => SoftwareDeinterlaceFilter::Yadif,
                    KnownVideoFilter::Bwdif => SoftwareDeinterlaceFilter::Bwdif,
                    KnownVideoFilter::W3fdif => SoftwareDeinterlaceFilter::W3fdif,
                    _ => return None,
                };
                return Some(
                    DeinterlaceFilter {
                        filter: software_filter,
                        input_is_interlaced: self.input_is_interlaced,
                    }
                    .into(),
                );
            }
        }

        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.is_interlaced = false;
        state.pixel_format = match state.pixel_format.bit_depth() {
            10 => PixelFormat::Yuv420p10le,
            _ => PixelFormat::Yuv420p,
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        match &self.filter {
            SoftwareDeinterlaceFilter::Yadif => Some(String::from("yadif=1")),
            SoftwareDeinterlaceFilter::Bwdif => Some(String::from("bwdif=1")),
            SoftwareDeinterlaceFilter::W3fdif => Some(String::from("w3fdif=1")),
        }
    }
}

#[derive(Clone)]
pub struct HwMapFilter {
    pub from_surface: FrameSurface,
    pub to_surface: FrameSurface,
    pub reverse: bool,
}

impl VideoFilterOp for HwMapFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.surface = self.to_surface;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        None
    }

    fn as_arg(&self) -> Option<String> {
        let reverse_part = if self.reverse { ":reverse=1" } else { "" };
        self.to_surface
            .device_name()
            .map(|name| format!("hwmap=derive_device={name}{reverse_part}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hw_map_as_arg_produces_derive_device() {
        let filter: VideoFilter = HwMapFilter {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::OpenCL,
            reverse: false,
        }
        .into();
        assert_eq!(
            filter.as_arg(),
            Some(String::from("hwmap=derive_device=opencl"))
        );
    }

    #[test]
    fn hw_map_as_arg_reverse_direction() {
        let filter: VideoFilter = HwMapFilter {
            from_surface: FrameSurface::OpenCL,
            to_surface: FrameSurface::Vaapi,
            reverse: true,
        }
        .into();
        assert_eq!(
            filter.as_arg(),
            Some(String::from("hwmap=derive_device=vaapi:reverse=1"))
        );
    }

    #[test]
    fn hw_map_as_arg_returns_none_for_system() {
        let filter: VideoFilter = HwMapFilter {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::System,
            reverse: false,
        }
        .into();
        assert_eq!(filter.as_arg(), None);
    }

    #[test]
    fn hw_map_apply_to_updates_surface() {
        let mut state = FrameState {
            size: FrameSize {
                width: 1920,
                height: 1080,
            },
            is_anamorphic: false,
            is_interlaced: false,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            surface: FrameSurface::Vaapi,
            pixel_format: PixelFormat::P010le,
            is_hdr: true,
        };

        let filter: VideoFilter = HwMapFilter {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::OpenCL,
            reverse: false,
        }
        .into();
        filter.apply_to(&mut state);

        assert_eq!(state.surface, FrameSurface::OpenCL);
        assert_eq!(state.pixel_format, PixelFormat::P010le);
        assert!(state.is_hdr);
    }

    #[test]
    fn hw_map_required_surface_is_none() {
        let filter: VideoFilter = HwMapFilter {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::OpenCL,
            reverse: false,
        }
        .into();
        assert_eq!(filter.required_surface(), None);
    }
}
