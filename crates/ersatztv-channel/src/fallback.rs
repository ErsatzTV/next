use std::fmt::{Display, Formatter};

use ersatztv_channel::error::ChannelError;
use ersatztv_playout::playout::PlayoutItem;
use time::OffsetDateTime;

#[derive(Debug)]
pub enum FallbackReason {
    /// no playout item covers the current time; the schedule resumes at `next_start`
    ScheduledGap { next_start: Option<OffsetDateTime> },

    /// no playout item could be selected (missing or invalid playout JSON, dynamic item failed, ...)
    ItemSelectionFailed(ChannelError),

    /// the playout item was selected but could not be transcoded
    TranscodeFailed {
        item_id: String,
        start: OffsetDateTime,
        finish: OffsetDateTime,
        error: ChannelError,
    },
}

impl FallbackReason {
    pub fn from_selection_error(error: ChannelError) -> FallbackReason {
        match error {
            ChannelError::PlayoutJsonNoItem { next_start } => {
                FallbackReason::ScheduledGap { next_start }
            }
            other => FallbackReason::ItemSelectionFailed(other),
        }
    }

    pub fn from_transcode_error(item: &PlayoutItem, error: ChannelError) -> FallbackReason {
        FallbackReason::TranscodeFailed {
            item_id: item.id.clone(),
            start: item.start,
            finish: item.finish,
            error,
        }
    }

    /// when the substituted content should end, if known
    pub fn fallback_until(&self) -> Option<OffsetDateTime> {
        match self {
            FallbackReason::ScheduledGap { next_start } => *next_start,
            FallbackReason::ItemSelectionFailed(_) => None,
            FallbackReason::TranscodeFailed { finish, .. } => Some(*finish),
        }
    }

    /// recover the underlying error, for modes that refuse to substitute content
    pub fn into_error(self) -> ChannelError {
        match self {
            FallbackReason::ScheduledGap { next_start } => {
                ChannelError::PlayoutJsonNoItem { next_start }
            }
            FallbackReason::ItemSelectionFailed(error) => error,
            FallbackReason::TranscodeFailed { error, .. } => error,
        }
    }

    pub fn log(&self, now: &OffsetDateTime) {
        match self {
            FallbackReason::ScheduledGap { next_start } => log::debug!(
                "no playout item covers {now}, replacing with black/silence until {}",
                next_start.map_or_else(|| String::from("the next reload"), |s| s.to_string())
            ),
            FallbackReason::ItemSelectionFailed(error) => log::error!(
                "no item could be selected for {now}, replacing with black/silence: {error}"
            ),
            FallbackReason::TranscodeFailed {
                item_id,
                start,
                finish,
                error,
            } => log::error!(
                "item {item_id} ({start} .. {finish}) failed, replacing with black/silence: {error}"
            ),
        }
    }
}

impl Display for FallbackReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FallbackReason::ScheduledGap {
                next_start: Some(next_start),
            } => {
                write!(f, "no playout item is scheduled until {next_start}")
            }
            FallbackReason::ScheduledGap { next_start: None } => {
                write!(f, "no playout item is scheduled")
            }
            FallbackReason::ItemSelectionFailed(error) => {
                write!(f, "unable to select playout item: {error}")
            }
            FallbackReason::TranscodeFailed { item_id, error, .. } => {
                write!(f, "playout item {item_id} failed to transcode: {error}")
            }
        }
    }
}

/// builds an ASS subtitle document that shows `message` centered near the bottom of the
/// frame for the whole fallback item
pub fn error_card_subtitle(message: &str, width: u32, height: u32) -> String {
    let font_size = (f64::from(height) / 20.0).round() as u32;
    let margin_v = (f64::from(height) * 0.05).round() as u32;
    let text = message.replace("\r\n", "\\N").replace('\n', "\\N");

    format!(
        "[Script Info]\n\
         ScriptType: v4.00+\n\
         WrapStyle: 0\n\
         ScaledBorderAndShadow: yes\n\
         YCbCr Matrix: None\n\
         PlayResX: {width}\n\
         PlayResY: {height}\n\
         \n\
         [V4+ Styles]\n\
         Format: Name, Fontname, Fontsize, PrimaryColour, OutlineColour, BorderStyle, Outline, Shadow, Alignment, Encoding\n\
         Style: Default,Roboto,{font_size},&HFFFFFF,,0,1,0,2,1\n\
         \n\
         [Events]\n\
         Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
         Dialogue: 0,0:00:00.00,99:99:99.99,Default,,0,0,{margin_v},,{text}\n"
    )
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn gap_ends_at_next_start() {
        let next_start = datetime!(2026-01-01 12:00 UTC);
        let reason = FallbackReason::from_selection_error(ChannelError::PlayoutJsonNoItem {
            next_start: Some(next_start),
        });

        assert_eq!(reason.fallback_until(), Some(next_start));
        assert!(matches!(
            reason.into_error(),
            ChannelError::PlayoutJsonNoItem { next_start: Some(s) } if s == next_start
        ));
    }

    #[test]
    fn error_card_subtitle_scales_with_height_and_escapes_newlines() {
        let ass = error_card_subtitle("line one\nline two", 1920, 1080);

        assert!(ass.contains("PlayResX: 1920\nPlayResY: 1080\n"));
        assert!(ass.contains("Style: Default,Roboto,54,"));
        assert!(
            ass.contains(
                "Dialogue: 0,0:00:00.00,99:99:99.99,Default,,0,0,54,,line one\\Nline two\n"
            )
        );
    }

    #[test]
    fn selection_error_has_unknown_end() {
        let reason = FallbackReason::from_selection_error(ChannelError::PlayoutJsonNoFileForTime(
            datetime!(2026-01-01 12:00 UTC),
        ));

        assert_eq!(reason.fallback_until(), None);
        assert!(matches!(
            reason.into_error(),
            ChannelError::PlayoutJsonNoFileForTime(_)
        ));
    }
}
