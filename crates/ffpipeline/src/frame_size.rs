use std::str::FromStr;

use crate::pipeline::FrameState;

#[derive(Debug, Clone, PartialEq)]
pub struct FrameSize {
    pub width: u32,
    pub height: u32,
}

impl FromStr for FrameSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('x');
        let (w, h) = match (parts.next(), parts.next(), parts.next()) {
            (Some(w), Some(h), None) => (w, h),
            _ => {
                return Err(format!(
                    "invalid frame size format: '{s}', expected 'WIDTHxHEIGHT'"
                ));
            }
        };
        let width = w
            .trim()
            .parse::<u32>()
            .map_err(|e| format!("invalid width '{w}': {e}"))?;
        let height = h
            .trim()
            .parse::<u32>()
            .map_err(|e| format!("invalid height '{h}': {e}"))?;
        Ok(FrameSize { width, height })
    }
}

impl FrameSize {
    pub(crate) fn square_pixel_size(&self, frame_state: &FrameState) -> FrameSize {
        let mut source_width = frame_state.size.width as f64;
        let mut source_height = frame_state.size.height as f64;

        if frame_state.is_anamorphic
            && let Some(sar) = Self::sar_as_float(frame_state)
        {
            source_width = (source_width * sar).floor();
            source_height = (source_height * sar).floor();
        }

        let target_width = self.width as f64;
        let target_height = self.height as f64;

        let width_percent = target_width / source_width;
        let height_percent = target_height / source_height;
        let min_percent = f64::min(width_percent, height_percent);

        FrameSize {
            width: (source_width * min_percent).floor() as u32,
            height: (source_height * min_percent).floor() as u32,
        }
    }

    fn sar_as_float(frame_state: &FrameState) -> Option<f64> {
        let sample_aspect_ratio: Option<&str> = frame_state
            .sample_aspect_ratio
            .as_ref()
            .map(|sar| sar.as_ref());

        // some media servers don't provide sample aspect ratio so we have to calculate it
        if sample_aspect_ratio.is_none() || sample_aspect_ratio == Some("0:0") {
            match &frame_state.display_aspect_ratio {
                Some(display_aspect_ratio) => {
                    // check for decimal DAR
                    match display_aspect_ratio.parse::<f64>() {
                        Ok(dar) => {
                            let res =
                                frame_state.size.width as f64 / frame_state.size.height as f64;
                            Some(dar / res)
                        }
                        Err(_) => {
                            // assume it's a ratio
                            Self::parse_ratio(display_aspect_ratio)
                        }
                    }
                }
                None => None,
            }
        } else if let Some(sample_aspect_ratio) = sample_aspect_ratio {
            Self::parse_ratio(sample_aspect_ratio)
        } else {
            None
        }
    }

    fn parse_ratio(ratio: &str) -> Option<f64> {
        let split: Vec<&str> = ratio.split(':').collect();
        if let Ok(num) = split[0].parse::<f64>()
            && let Ok(den) = split[1].parse::<f64>()
        {
            Some(num / den)
        } else {
            None
        }
    }
}
