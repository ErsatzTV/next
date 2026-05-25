use enum_dispatch::enum_dispatch;

use crate::accel::cuda::CudaOverlay;
use crate::accel::vaapi::VaapiOverlay;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat};
use crate::video_filter::VideoFilter;

#[derive(Debug, Clone)]
pub struct OverlayFilter {
    pub kind: OverlayKind,
    pub secondary: Vec<VideoFilter>,
    pub secondary_initial_state: FrameState,
    pub secondary_source: OverlaySource,
    pub location: Option<FramePoint>,
}

impl OverlayFilter {
    pub fn with_kind(&self, kind: OverlayKind) -> OverlayFilter {
        OverlayFilter {
            kind,
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone)]
pub struct FramePoint {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone)]
pub enum OverlaySource {
    Subtitle,
    Watermark,
}

#[derive(Debug, Clone)]
#[enum_dispatch(OverlayKindOp)]
pub enum OverlayKind {
    Software(SoftwareOverlay),
    Cuda(CudaOverlay),
    Vaapi(VaapiOverlay),
}

#[derive(Debug, Clone)]
pub struct SoftwareOverlay {
    pub bit_depth: u8,
}

impl Default for SoftwareOverlay {
    fn default() -> Self {
        Self { bit_depth: 8 }
    }
}

impl OverlayKindOp for SoftwareOverlay {
    fn apply_to(&self, state: &mut FrameState) {
        match self.bit_depth {
            10 => state.pixel_format = PixelFormat::Yuv420p10le,
            _ => state.pixel_format = PixelFormat::Yuv420p,
        }
    }

    fn main_input_state(&self, current_state: &FrameState) -> FrameState {
        FrameState {
            surface: FrameSurface::System,
            pixel_format: match current_state.pixel_format.bit_depth() {
                10 => PixelFormat::Yuv420p10le,
                _ => PixelFormat::Yuv420p,
            },
            ..current_state.clone()
        }
    }

    fn secondary_input_state(&self, current_state: &FrameState) -> FrameState {
        FrameState {
            surface: FrameSurface::System,
            pixel_format: match current_state.pixel_format.bit_depth() {
                10 => PixelFormat::Yuva420p10le,
                _ => PixelFormat::Yuva420p,
            },
            ..current_state.clone()
        }
    }

    fn as_arg(&self, location: Option<FramePoint>) -> Option<String> {
        let fmt = match self.bit_depth {
            10 => "1",
            _ => "0",
        };

        if let Some(location) = location {
            Some(format!(
                "overlay=x={}:y={}:format={fmt}",
                location.x, location.y
            ))
        } else {
            Some(format!("overlay=x=(W-w)/2:y=(H-h)/2:format={fmt}"))
        }
    }

    fn configure(&mut self, main: &FrameState) {
        self.bit_depth = main.pixel_format.bit_depth();
    }
}

#[enum_dispatch]
pub trait OverlayKindOp {
    fn apply_to(&self, state: &mut FrameState);
    fn main_input_state(&self, current_state: &FrameState) -> FrameState;
    fn secondary_input_state(&self, current_state: &FrameState) -> FrameState;
    fn as_arg(&self, location: Option<FramePoint>) -> Option<String>;

    /// capture anything needed from main state (e.g. bit depth)
    fn configure(&mut self, _main: &FrameState) {}
}
