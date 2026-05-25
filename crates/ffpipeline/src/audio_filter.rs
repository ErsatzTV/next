use crate::output_settings::AudioLoudnessSettings;
use crate::pipeline::{FrameState, Hz};

#[derive(Debug, Clone)]
pub enum AudioFilter {
    Resample,
    Pad,
    LoudNorm {
        settings: Option<AudioLoudnessSettings>,
        sample_rate: Option<Hz>,
    },
}

impl AudioFilter {
    /// Determines whether the filter is needed given the input frame state. If so, the filter
    /// and its output frame state will be returned.
    pub(crate) fn evaluate(&self, _state: &FrameState) -> Option<AudioFilter> {
        Some(self.clone())
    }

    pub(crate) fn apply_to(&self, _state: &mut FrameState) {}

    pub(crate) fn as_arg(&self) -> Option<String> {
        match self {
            AudioFilter::Resample => Some(String::from("aresample=async=1")),
            AudioFilter::Pad => Some(String::from("apad")),
            AudioFilter::LoudNorm {
                settings,
                sample_rate,
            } => settings.as_ref().map(|s| {
                format!(
                    "loudnorm=I={}:TP={}:LRA={},aresample={}",
                    s.integrated_target,
                    s.true_peak,
                    s.range_target,
                    sample_rate.unwrap_or(Hz(48_000)).0,
                )
            }),
        }
    }
}
