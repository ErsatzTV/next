use enum_dispatch::enum_dispatch;

use crate::pipeline::{FrameState, FrameSurface, PixelFormat};
use crate::video_filter::VideoFilter;

#[derive(Clone)]
#[enum_dispatch(OverlayFilterOp)]
pub enum OverlayFilter {
    Overlay(SoftwareOverlayFilter),
}

#[derive(Clone)]
pub struct SoftwareOverlayFilter {
    pub secondary: Vec<VideoFilter>,
}

impl OverlayFilterOp for SoftwareOverlayFilter {
    fn secondary(&self) -> &Vec<VideoFilter> {
        self.secondary.as_ref()
    }

    fn replace_secondary(&mut self, secondary: Vec<VideoFilter>) {
        self.secondary = secondary;
    }

    fn secondary_initial_state(&self) -> FrameState {
        FrameState::default()
    }

    fn apply_to(&self, _state: &mut FrameState) {}

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
                10 => PixelFormat::Yuv420p10le,
                _ => PixelFormat::Yuv420p,
            },
            ..FrameState::default()
        }
    }

    fn as_arg(&self) -> Option<String> {
        Some(String::from("overlay"))
    }
}

#[enum_dispatch]
pub trait OverlayFilterOp {
    fn secondary(&self) -> &Vec<VideoFilter>;
    fn replace_secondary(&mut self, secondary: Vec<VideoFilter>);
    fn secondary_initial_state(&self) -> FrameState;

    fn apply_to(&self, state: &mut FrameState);
    fn main_input_state(&self, current_state: &FrameState) -> FrameState;
    fn secondary_input_state(&self, current_state: &FrameState) -> FrameState;
    fn as_arg(&self) -> Option<String>;
}
