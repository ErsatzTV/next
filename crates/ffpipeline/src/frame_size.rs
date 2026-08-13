use std::str::FromStr;

use derive_more::Display;

use crate::pipeline::FrameState;

#[derive(Debug, Clone, Copy, PartialEq, Display)]
#[display("FrameSize(w={},h={})", width, height)]
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
    pub(crate) fn square_pixel_size_contain(&self, frame_state: &FrameState) -> FrameSize {
        let mut source_width = frame_state.size.width as f64;
        let source_height = frame_state.size.height as f64;

        if frame_state.is_anamorphic
            && let Some(sar) = Self::sar_as_float(frame_state)
        {
            source_width *= sar;
        }

        let min_percent = f64::min(
            self.width as f64 / source_width,
            self.height as f64 / source_height,
        );

        let width = ((source_width * min_percent).round_ties_even() as u32).min(self.width);
        let height = ((source_height * min_percent).round_ties_even() as u32).min(self.height);

        FrameSize {
            width: width - (width % 2),
            height: height - (height % 2),
        }
    }

    pub(crate) fn square_pixel_size_cover(&self, frame_state: &FrameState) -> FrameSize {
        let mut source_width = frame_state.size.width as f64;
        let source_height = frame_state.size.height as f64;

        if frame_state.is_anamorphic
            && let Some(sar) = Self::sar_as_float(frame_state)
        {
            source_width *= sar;
        }

        let max_percent = f64::max(
            self.width as f64 / source_width,
            self.height as f64 / source_height,
        );

        let width = (source_width * max_percent).round_ties_even() as u32;
        let height = (source_height * max_percent).round_ties_even() as u32;

        FrameSize {
            width: width - (width % 2),
            height: height - (height % 2),
        }
    }

    fn sar_as_float(frame_state: &FrameState) -> Option<f64> {
        let sar = frame_state.sample_aspect_ratio.as_deref();
        if !is_unspecified_ratio(sar) {
            return parse_aspect_ratio(sar?);
        }

        // some media servers don't provide sample aspect ratio so we have to calculate it
        let dar = parse_aspect_ratio(frame_state.display_aspect_ratio.as_deref()?)?;
        let storage = storage_aspect_ratio(frame_state.size.width, frame_state.size.height)?;
        Some(dar / storage)
    }
}

/// ffmpeg and media servers are inconsistent about how they report an unknown aspect ratio
pub(crate) fn is_unspecified_ratio(ratio: Option<&str>) -> bool {
    match ratio {
        Some(ratio) => {
            let ratio = ratio.trim();
            ratio.is_empty() || ratio == "0:0" || ratio == "0:1"
        }
        None => true,
    }
}

/// aspect ratios are reported either as a ratio ("16:9") or as a decimal ("1.777778")
pub(crate) fn parse_aspect_ratio(ratio: &str) -> Option<f64> {
    let value = match ratio.split_once(':') {
        Some((num, den)) => num.trim().parse::<f64>().ok()? / den.trim().parse::<f64>().ok()?,
        None => ratio.trim().parse::<f64>().ok()?,
    };

    (value.is_finite() && value > 0f64).then_some(value)
}

pub(crate) fn storage_aspect_ratio(width: u32, height: u32) -> Option<f64> {
    (width > 0 && height > 0).then(|| width as f64 / height as f64)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::pipeline::{FrameSurface, HdrFormat, PixelFormat};

    #[test]
    fn anamorphic_square_pixels_1280x720() {
        let state = FrameState {
            size: FrameSize {
                width: 720,
                height: 480,
            },
            is_anamorphic: true,
            is_interlaced: false,
            sample_aspect_ratio: Some(String::from("32:27")),
            display_aspect_ratio: Some(String::from("16:9")),
            surface: FrameSurface::System,
            pixel_format: PixelFormat::Yuv420p,
            hdr_format: HdrFormat::None,
        };

        let target = FrameSize {
            width: 1280,
            height: 720,
        };

        assert_eq!(target.square_pixel_size_contain(&state), target);
    }

    #[test]
    fn anamorphic_square_pixels_1920x1080() {
        let state = FrameState {
            size: FrameSize {
                width: 720,
                height: 480,
            },
            is_anamorphic: true,
            is_interlaced: false,
            sample_aspect_ratio: Some(String::from("32:27")),
            display_aspect_ratio: Some(String::from("16:9")),
            surface: FrameSurface::System,
            pixel_format: PixelFormat::Yuv420p,
            hdr_format: HdrFormat::None,
        };

        let target = FrameSize {
            width: 1920,
            height: 1080,
        };

        assert_eq!(target.square_pixel_size_contain(&state), target);
    }

    /// a missing SAR must be derived from the DAR; using the DAR as if it were the SAR stretched
    /// 720x480 to 1280x480 instead of 1280x720
    #[rstest]
    #[case(None, Some("16:9"))]
    #[case(Some(""), Some("16:9"))]
    #[case(Some("0:0"), Some("16:9"))]
    #[case(Some("0:1"), Some("16:9"))]
    #[case(Some("0:0"), Some("1.777778"))]
    fn unspecified_sar_derives_from_dar(
        #[case] sample_aspect_ratio: Option<&str>,
        #[case] display_aspect_ratio: Option<&str>,
    ) {
        let state = FrameState {
            size: FrameSize {
                width: 720,
                height: 480,
            },
            is_anamorphic: true,
            is_interlaced: false,
            sample_aspect_ratio: sample_aspect_ratio.map(String::from),
            display_aspect_ratio: display_aspect_ratio.map(String::from),
            surface: FrameSurface::System,
            pixel_format: PixelFormat::Yuv420p,
            hdr_format: HdrFormat::None,
        };

        let target = FrameSize {
            width: 1280,
            height: 720,
        };

        assert_eq!(target.square_pixel_size_contain(&state), target);
    }

    /// with nothing usable to derive a SAR from, the source size must be left alone
    #[rstest]
    #[case(Some("0:0"), None)]
    #[case(Some("0:0"), Some("0:1"))]
    #[case(Some("0:0"), Some("0:0"))]
    #[case(Some("junk"), Some("junk"))]
    fn unspecified_sar_and_dar_ignores_sar(
        #[case] sample_aspect_ratio: Option<&str>,
        #[case] display_aspect_ratio: Option<&str>,
    ) {
        let state = FrameState {
            size: FrameSize {
                width: 720,
                height: 480,
            },
            is_anamorphic: true,
            is_interlaced: false,
            sample_aspect_ratio: sample_aspect_ratio.map(String::from),
            display_aspect_ratio: display_aspect_ratio.map(String::from),
            surface: FrameSurface::System,
            pixel_format: PixelFormat::Yuv420p,
            hdr_format: HdrFormat::None,
        };

        let target = FrameSize {
            width: 1280,
            height: 720,
        };

        // 720x480 fitted into 1280x720 as-is
        assert_eq!(
            target.square_pixel_size_contain(&state),
            FrameSize {
                width: 1080,
                height: 720
            }
        );
    }

    #[rstest]
    #[case("16:9", Some(16f64 / 9f64))]
    #[case("32:27", Some(32f64 / 27f64))]
    #[case(" 4 : 3 ", Some(4f64 / 3f64))]
    #[case("1.777778", Some(1.777778f64))]
    #[case("0:0", None)]
    #[case("0:1", None)]
    #[case("1:0", None)]
    #[case("16", Some(16f64))]
    #[case("16:9:2", None)]
    #[case("-16:9", None)]
    #[case("", None)]
    #[case("junk", None)]
    fn parse_aspect_ratio_cases(#[case] input: &str, #[case] expected: Option<f64>) {
        assert_eq!(parse_aspect_ratio(input), expected);
    }

    #[rstest]
    fn round_down_to_even_contain() {
        let state = FrameState {
            size: FrameSize {
                width: 1920,
                height: 1036,
            },
            is_anamorphic: false,
            is_interlaced: false,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            surface: FrameSurface::System,
            pixel_format: PixelFormat::Yuv420p,
            hdr_format: HdrFormat::None,
        };

        let target = FrameSize {
            width: 1280,
            height: 720,
        };

        assert_eq!(
            target.square_pixel_size_contain(&state),
            FrameSize {
                width: 1280,
                height: 690
            }
        );
    }

    #[rstest]
    fn round_down_to_even_cover() {
        let state = FrameState {
            size: FrameSize {
                width: 1902,
                height: 1038,
            },
            is_anamorphic: false,
            is_interlaced: false,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            surface: FrameSurface::System,
            pixel_format: PixelFormat::Yuv420p,
            hdr_format: HdrFormat::None,
        };

        let target = FrameSize {
            width: 1280,
            height: 720,
        };

        assert_eq!(
            target.square_pixel_size_cover(&state),
            FrameSize {
                width: 1318,
                height: 720
            }
        );
    }
}
