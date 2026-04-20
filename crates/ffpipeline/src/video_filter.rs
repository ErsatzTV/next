use dyn_clone::DynClone;

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

pub trait HwVideoFilter: DynClone {
    fn evaluate(&self, state: &FrameState) -> Option<VideoFilter>;
    fn apply_to(&self, state: &mut FrameState);
    fn required_surface(&self) -> FrameSurface;
    fn as_arg(&self) -> Option<String>;
}

dyn_clone::clone_trait_object!(HwVideoFilter);

#[derive(Clone)]
pub enum VideoFilter {
    HwUpload {
        target_surface: FrameSurface,
        source_format: PixelFormat,
    },
    HwDownload {
        target_pixel_format: PixelFormat,
    },
    Scale {
        size: Option<FrameSize>,
        input_is_anamorphic: bool,
        force_original_aspect_ratio: Option<ForceOriginalAspectRatio>,
    },
    Pad {
        size: Option<FrameSize>,
    },
    Loop {
        codec: String,
    },
    Format {
        format: PixelFormat,
    },
    ToneMap {
        algorithm: Option<String>,
        format: PixelFormat,
    },
    Deinterlace {
        filter: SoftwareDeinterlaceFilter,
        input_is_interlaced: bool,
    },
    Hardware(Box<dyn HwVideoFilter>),
}

impl VideoFilter {
    /// Determines whether the filter is needed given the input frame state.
    pub(crate) fn evaluate(
        &self,
        state: &FrameState,
        ffmpeg_info: &FfmpegInfo,
    ) -> Option<VideoFilter> {
        match self {
            // hardware filters aren't present at the point where this is called
            VideoFilter::HwUpload { .. } => None,
            VideoFilter::HwDownload { .. } => None,
            VideoFilter::Format { .. } => None,

            VideoFilter::Hardware(filter) => filter.evaluate(state),

            VideoFilter::Scale {
                size: Some(target), ..
            } => {
                if state.size == *target {
                    None
                } else {
                    let actual = target.square_pixel_size(state);
                    let force_original_aspect_ratio = if actual == *target {
                        None
                    } else {
                        Some(ForceOriginalAspectRatio::Decrease)
                    };

                    Some(VideoFilter::Scale {
                        size: Some(actual.clone()),
                        input_is_anamorphic: state.is_anamorphic,
                        force_original_aspect_ratio,
                    })
                }
            }
            VideoFilter::Scale { size: None, .. } => None,
            VideoFilter::Pad { size: Some(target) } => {
                if state.size == *target {
                    None
                } else {
                    Some(self.clone())
                }
            }
            VideoFilter::Pad { size: None } => None,
            VideoFilter::Loop { codec } if codec == "png" => Some(self.clone()),
            VideoFilter::Loop { .. } => None,
            VideoFilter::ToneMap { .. } => {
                if !state.is_hdr {
                    None
                } else {
                    Some(self.clone())
                }
            }
            VideoFilter::Deinterlace {
                input_is_interlaced,
                ..
            } => {
                if *input_is_interlaced {
                    let mut filter_options = [
                        (KnownVideoFilter::Yadif, SoftwareDeinterlaceFilter::Yadif),
                        (KnownVideoFilter::Bwdif, SoftwareDeinterlaceFilter::Bwdif),
                        (KnownVideoFilter::W3fdif, SoftwareDeinterlaceFilter::W3fdif),
                    ];

                    filter_options.sort_by_key(|(k, _)| {
                        ffmpeg_info
                            .preferred_filters
                            .iter()
                            .position(|p| p == &k.to_string())
                            .unwrap_or(usize::MAX)
                    });

                    for (known_filter, software_filter) in filter_options {
                        if ffmpeg_info.has_video_filter(&known_filter) {
                            return Some(VideoFilter::Deinterlace {
                                filter: software_filter,
                                input_is_interlaced: *input_is_interlaced,
                            });
                        }
                    }
                }

                None
            }
        }
    }

    pub(crate) fn apply_to(&self, state: &mut FrameState) {
        match self {
            VideoFilter::HwUpload { target_surface, .. } => {
                state.surface = target_surface.clone();
                state.pixel_format = match &state.pixel_format {
                    PixelFormat::Yuv420p => PixelFormat::Nv12,
                    PixelFormat::Yuv420p10le => PixelFormat::P010le,
                    other => other.clone(),
                }
            }
            VideoFilter::HwDownload {
                target_pixel_format,
            } => {
                state.surface = FrameSurface::System;
                state.pixel_format = target_pixel_format.clone();
            }
            VideoFilter::Hardware(hardware_filter) => {
                hardware_filter.apply_to(state);
            }
            VideoFilter::Scale {
                size: Some(size), ..
            } => {
                state.size = size.clone();
                state.is_anamorphic = false;
                state.sample_aspect_ratio = Some(String::from("1:1"));
                state.display_aspect_ratio = None;
            }
            VideoFilter::Scale { size: None, .. } => {}
            VideoFilter::Pad { size: Some(size) } => {
                state.size = size.clone();
                state.surface = FrameSurface::System;
            }
            VideoFilter::Pad { size: None } => {}
            VideoFilter::Loop { .. } => {}
            VideoFilter::Format { format } => {
                state.pixel_format = format.clone();
            }
            VideoFilter::ToneMap { format, .. } => {
                state.pixel_format = format.clone();
                state.is_hdr = false;
            }
            VideoFilter::Deinterlace { .. } => {
                state.is_interlaced = false;
                state.pixel_format = match state.pixel_format.bit_depth() {
                    10 => PixelFormat::Yuv420p10le,
                    _ => PixelFormat::Yuv420p,
                }
            }
        }
    }

    pub(crate) fn required_surface(&self) -> Option<FrameSurface> {
        match self {
            VideoFilter::HwUpload { .. } => None,
            VideoFilter::HwDownload { .. } => None,
            VideoFilter::Hardware(hardware_filter) => Some(hardware_filter.required_surface()),

            VideoFilter::Scale { .. } => Some(FrameSurface::System),
            VideoFilter::Pad { .. } => Some(FrameSurface::System),
            VideoFilter::Loop { .. } => Some(FrameSurface::System),
            VideoFilter::Format { .. } => Some(FrameSurface::System),
            VideoFilter::ToneMap { .. } => Some(FrameSurface::System),
            VideoFilter::Deinterlace { .. } => Some(FrameSurface::System),
        }
    }

    pub(crate) fn as_arg(&self) -> Option<String> {
        match self {
            VideoFilter::HwUpload {
                target_surface,
                source_format,
            } => {
                let target_format = match source_format.bit_depth() {
                    10 => PixelFormat::P010le,
                    _ => PixelFormat::Nv12,
                };

                let format_filter = if source_format == &target_format {
                    String::new()
                } else {
                    format!("format={},", target_format.as_arg())
                };

                match target_surface {
                    // TODO: refactor this into each hwaccel, maybe a hw_upload_filter() fn?
                    FrameSurface::Cuda => Some(format!("{format_filter}hwupload_cuda")),
                    FrameSurface::Qsv => {
                        Some(format!("{format_filter}hwupload=extra_hw_frames=64"))
                    }
                    FrameSurface::Vaapi => Some(format!("{format_filter}hwupload")),
                    FrameSurface::Vulkan => Some(format!("{format_filter}hwupload")),
                    _ => None,
                }
            }
            VideoFilter::HwDownload {
                target_pixel_format,
            } => Some(format!(
                "hwdownload,format={}",
                target_pixel_format.as_arg()
            )),
            VideoFilter::Hardware(hardware_filter) => hardware_filter.as_arg(),
            VideoFilter::Scale {
                size: Some(size),
                input_is_anamorphic,
                force_original_aspect_ratio,
            } => {
                let aspect_ratio = force_original_aspect_ratio
                    .as_ref()
                    .map_or(String::new(), |f| f.as_arg());

                if *input_is_anamorphic {
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
            }
            VideoFilter::Scale { .. } => None,
            VideoFilter::Pad { size: Some(size) } => Some(format!(
                "pad={}:{}:-1:-1:color=black",
                size.width, size.height
            )),
            VideoFilter::Pad { .. } => None,
            VideoFilter::Loop { .. } => Some(String::from("loop=-1:1")),
            VideoFilter::Format { format } => Some(format!("format={}", format.as_arg())),
            VideoFilter::ToneMap { algorithm, format } => Some(format!(
                "zscale=transfer=linear,tonemap={},zscale=transfer=bt709,format={}",
                algorithm.as_deref().unwrap_or("linear"),
                format.as_arg()
            )),
            VideoFilter::Deinterlace {
                filter: SoftwareDeinterlaceFilter::Yadif,
                ..
            } => Some(String::from("yadif=1")),
            VideoFilter::Deinterlace {
                filter: SoftwareDeinterlaceFilter::Bwdif,
                ..
            } => Some(String::from("bwdif=1")),
            VideoFilter::Deinterlace {
                filter: SoftwareDeinterlaceFilter::W3fdif,
                ..
            } => Some(String::from("w3fdif=1")),
        }
    }
}
