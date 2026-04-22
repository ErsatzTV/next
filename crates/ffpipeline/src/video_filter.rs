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

impl TryFrom<&KnownVideoFilter> for SoftwareDeinterlaceFilter {
    type Error = String;
    fn try_from(known_filter: &KnownVideoFilter) -> Result<SoftwareDeinterlaceFilter, Self::Error> {
        match known_filter {
            KnownVideoFilter::Bwdif => Ok(SoftwareDeinterlaceFilter::Bwdif),
            KnownVideoFilter::Yadif => Ok(SoftwareDeinterlaceFilter::Yadif),
            KnownVideoFilter::W3fdif => Ok(SoftwareDeinterlaceFilter::W3fdif),
            _ => Err(format!(
                "Unknown software deinterlace filter: {}",
                known_filter
            )),
        }
    }
}

pub trait HwVideoFilter: DynClone {
    /// Evaluates the current state of a video frame and determines the
    /// appropriate video filter to be applied.
    ///
    /// # Parameters
    /// - `&self`: A reference to the instance of the containing type.
    /// - `state`: A reference to the current `FrameState` which contains
    ///   information about the video frame to be evaluated.
    ///
    /// # Returns
    /// - `Option<VideoFilter>`: Returns `Some(VideoFilter)` if a suitable video
    ///   filter is determined based on the frame state. Returns `None` if no filter
    ///   is applicable.
    ///
    /// # Usage
    /// This function is designed to analyze the provided `FrameState` to determine
    /// whether a filter should be applied to the video frame and, if so, what type
    /// of filter it should be. It returns `None` when no filtering logic is necessary.
    ///
    /// # Example
    /// ```rust,ignore
    /// let frame_state = FrameState::new(...);
    /// let filter = hw_filter.evaluate(&frame_state);
    /// ```
    fn evaluate(&self, state: &FrameState) -> Option<VideoFilter>;
    /// Applies the current object's logic or transformations to the given `FrameState` object.
    ///
    /// This method modifies the provided `FrameState` in place. Implementations of this function
    /// define the specific behavior to be applied to the state, such as updating properties,
    /// performing calculations, or appending data.
    ///
    /// # Parameters
    /// - `state`: A mutable reference to a `FrameState` object that represents the current state
    ///   of the frame. The function alters this object directly.
    ///
    /// # Example
    /// ```rust,ignore
    /// let mut frame_state = FrameState::new();
    /// some_object.apply_to(&mut frame_state);
    /// // The frame_state is now modified based on the logic of `apply_to`.
    /// ```
    ///
    /// # Note
    /// This method assumes that the caller has properly instantiated and initialized the
    /// `FrameState` object before invoking this function.
    fn apply_to(&self, state: &mut FrameState);
    /// Returns the `FrameSurface` that is required for the current context.
    ///
    /// This method determines and returns the necessary `FrameSurface` that
    /// corresponds to the operations or rendering tasks associated with the
    /// object it is called on.
    ///
    /// # Returns
    ///
    /// * `FrameSurface` - Specifies the type of frame surface that is necessary for this filter's operations
    ///   to be executed, based on the underlying implementation or hardware requirements.
    fn required_surface(&self) -> FrameSurface;
    /// Converts the implementer into an optional `String` representation.
    ///
    /// # Returns
    /// - `Some(String)` if the implementer can be represented as a `String`.
    /// - `None` if the implementer cannot be converted to a `String`.
    ///
    /// # Examples
    /// ```rust
    /// struct MyType;
    ///
    /// impl MyType {
    ///     fn as_arg(&self) -> Option<String> {
    ///         Some("example".to_string())
    ///     }
    /// }
    ///
    /// let my_obj = MyType;
    /// assert_eq!(my_obj.as_arg(), Some("example".to_string()));
    /// ```
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
    HwMap {
        from_surface: FrameSurface,
        to_surface: FrameSurface,
        reverse: bool,
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
            VideoFilter::HwMap { .. } => None,
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
                    let best_software_deinterlace_filter = ffmpeg_info
                        .find_best_fit(&[
                            KnownVideoFilter::Yadif,
                            KnownVideoFilter::Bwdif,
                            KnownVideoFilter::W3fdif,
                        ])
                        .and_then(|known_filter| {
                            SoftwareDeinterlaceFilter::try_from(known_filter).ok()
                        });

                    if let Some(best_software_deinterlace_filter) = best_software_deinterlace_filter
                    {
                        return Some(VideoFilter::Deinterlace {
                            filter: best_software_deinterlace_filter,
                            input_is_interlaced: *input_is_interlaced,
                        });
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
            VideoFilter::HwMap { to_surface, .. } => {
                state.surface = to_surface.clone();
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
            VideoFilter::HwMap { .. } => None,
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
            VideoFilter::HwMap {
                to_surface,
                reverse,
                ..
            } => {
                let reverse_part = if *reverse { ":reverse=1" } else { "" };
                to_surface
                    .device_name()
                    .map(|name| format!("hwmap=derive_device={name}{reverse_part}"))
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hw_map_as_arg_produces_derive_device() {
        let filter = VideoFilter::HwMap {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::OpenCL,
            reverse: false,
        };
        assert_eq!(
            filter.as_arg(),
            Some(String::from("hwmap=derive_device=opencl"))
        );
    }

    #[test]
    fn hw_map_as_arg_reverse_direction() {
        let filter = VideoFilter::HwMap {
            from_surface: FrameSurface::OpenCL,
            to_surface: FrameSurface::Vaapi,
            reverse: true,
        };
        assert_eq!(
            filter.as_arg(),
            Some(String::from("hwmap=derive_device=vaapi:reverse=1"))
        );
    }

    #[test]
    fn hw_map_as_arg_returns_none_for_system() {
        let filter = VideoFilter::HwMap {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::System,
            reverse: false,
        };
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

        let filter = VideoFilter::HwMap {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::OpenCL,
            reverse: false,
        };
        filter.apply_to(&mut state);

        assert_eq!(state.surface, FrameSurface::OpenCL);
        assert_eq!(state.pixel_format, PixelFormat::P010le);
        assert!(state.is_hdr);
    }

    #[test]
    fn hw_map_required_surface_is_none() {
        let filter = VideoFilter::HwMap {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::OpenCL,
            reverse: false,
        };
        assert_eq!(filter.required_surface(), None);
    }
}
