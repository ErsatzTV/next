use enum_dispatch::enum_dispatch;

use crate::pipeline::{FrameState, FrameSurface, PixelFormat};
use crate::video_filter::VideoFilter;

#[derive(Clone)]
pub struct OverlayFilter {
    pub kind: OverlayKind,
    pub secondary: Vec<VideoFilter>,
    pub secondary_initial_state: FrameState,
}

#[derive(Clone)]
#[enum_dispatch(OverlayKindOp)]
pub enum OverlayKind {
    Software(SoftwareOverlay),
}

#[derive(Clone)]
pub struct SoftwareOverlay;

impl OverlayKindOp for SoftwareOverlay {
    fn apply_to(&self, _state: &mut FrameState) {
        // no change to state when applying software overlay
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
            ..FrameState::default()
        }
    }

    fn as_arg(&self) -> Option<String> {
        Some(String::from("overlay"))
    }
}

#[enum_dispatch]
pub trait OverlayKindOp {
    fn apply_to(&self, state: &mut FrameState);
    fn main_input_state(&self, current_state: &FrameState) -> FrameState;
    fn secondary_input_state(&self, current_state: &FrameState) -> FrameState;
    fn as_arg(&self) -> Option<String>;
}
