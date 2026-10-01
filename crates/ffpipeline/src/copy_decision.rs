use std::fmt;

use crate::input::{InputSource, ProbedInput};
use crate::output_settings::CopyPolicy;
use crate::pipeline::VideoFormat;
use crate::probe::{ProbeResultAudioStream, ProbeResultVideoStream};

#[derive(Debug, Clone, PartialEq)]
pub enum CopyDecision {
    Copy,
    Transcode(Vec<CopyBlocker>),
}

impl CopyDecision {
    fn from_blockers(blockers: Vec<CopyBlocker>) -> Self {
        if blockers.is_empty() {
            CopyDecision::Copy
        } else {
            CopyDecision::Transcode(blockers)
        }
    }

    pub fn is_copy(&self) -> bool {
        matches!(self, CopyDecision::Copy)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CopyBlocker {
    CodecNotAllowed(String),
    StillImage,
    GraphicsLayers,
    ImageSubtitle,
    BurnedSubtitle,
    DolbyVision5,
    GeneratedSource,
    /// AVI packets carry no pts, so a copied timeline drifts; AAC copy from AVI into mpegts fails
    ContainerWithoutPts,
}

impl fmt::Display for CopyBlocker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CopyBlocker::CodecNotAllowed(codec) => write!(f, "{codec} is not in copy_formats"),
            CopyBlocker::StillImage => f.write_str("still image"),
            CopyBlocker::GraphicsLayers => f.write_str("graphics layers"),
            CopyBlocker::ImageSubtitle => f.write_str("image subtitles"),
            CopyBlocker::BurnedSubtitle => f.write_str("burned-in text subtitles"),
            CopyBlocker::DolbyVision5 => f.write_str("dolby vision profile 5"),
            CopyBlocker::GeneratedSource => f.write_str("generated (lavfi) source"),
            CopyBlocker::ContainerWithoutPts => f.write_str("avi container"),
        }
    }
}

/// One decision per stream the channel wants to copy; `None` means that stream always transcodes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CopyDecisions {
    pub video: Option<CopyDecision>,
    pub audio: Option<CopyDecision>,
}

impl CopyDecisions {
    /// Names each stream that wanted to copy but has to transcode, with its blockers.
    pub fn transcode_summary(&self) -> Option<String> {
        let streams: Vec<String> = [("video", &self.video), ("audio", &self.audio)]
            .into_iter()
            .filter_map(|(stream, decision)| match decision {
                Some(CopyDecision::Transcode(blockers)) => Some(format!(
                    "{stream} ({})",
                    blockers
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
                _ => None,
            })
            .collect();

        (!streams.is_empty()).then(|| streams.join("; "))
    }
}

/// Video filters that a copied stream can't carry.
#[derive(Debug, Default)]
pub(crate) struct VideoCopyContext {
    pub(crate) is_still_image: bool,
    pub(crate) has_graphics: bool,
    pub(crate) image_subtitle: bool,
    pub(crate) burned_subtitle: bool,
}

pub(crate) fn video_copy_decision(
    policy: &CopyPolicy<VideoFormat>,
    input: &ProbedInput,
    stream: &ProbeResultVideoStream,
    context: &VideoCopyContext,
) -> CopyDecision {
    let mut blockers = source_blockers(input);

    let allowed = stream
        .codec
        .parse::<VideoFormat>()
        .is_ok_and(|format| policy.formats.contains(&format));
    if !allowed {
        blockers.push(CopyBlocker::CodecNotAllowed(stream.codec.clone()));
    }

    for (blocked, blocker) in [
        (context.is_still_image, CopyBlocker::StillImage),
        (stream.dv_profile == Some(5), CopyBlocker::DolbyVision5),
        (context.has_graphics, CopyBlocker::GraphicsLayers),
        (context.image_subtitle, CopyBlocker::ImageSubtitle),
        (context.burned_subtitle, CopyBlocker::BurnedSubtitle),
    ] {
        if blocked {
            blockers.push(blocker);
        }
    }

    CopyDecision::from_blockers(blockers)
}

pub(crate) fn audio_copy_decision(
    policy: &CopyPolicy<String>,
    input: &ProbedInput,
    stream: &ProbeResultAudioStream,
) -> CopyDecision {
    let mut blockers = source_blockers(input);

    if !policy.formats.contains(&stream.codec) {
        blockers.push(CopyBlocker::CodecNotAllowed(stream.codec.clone()));
    }

    CopyDecision::from_blockers(blockers)
}

fn source_blockers(input: &ProbedInput) -> Vec<CopyBlocker> {
    let mut blockers = Vec::new();
    if matches!(input.input_source, InputSource::Lavfi(_)) {
        blockers.push(CopyBlocker::GeneratedSource);
    }
    if input.probe_result.format_name.as_deref() == Some("avi") {
        blockers.push(CopyBlocker::ContainerWithoutPts);
    }
    blockers
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::frame_rate::FrameRate;
    use crate::input::{LavfiInputSource, LocalInputSource};
    use crate::probe::{CodecType, ProbeResult};

    fn input(source: InputSource, format_name: &str) -> ProbedInput {
        ProbedInput {
            input_source: source,
            probe_result: ProbeResult {
                path: String::from("source"),
                streams: Vec::new(),
                duration: Some(Duration::from_secs(60)),
                format_name: Some(format_name.to_owned()),
            },
            in_point: Duration::ZERO,
            out_point: Duration::from_secs(60),
            stream_index: None,
        }
    }

    fn local(format_name: &str) -> ProbedInput {
        input(
            InputSource::Local(LocalInputSource {
                path: String::from("source"),
            }),
            format_name,
        )
    }

    fn lavfi() -> ProbedInput {
        input(
            InputSource::Lavfi(LavfiInputSource {
                params: String::from("anullsrc"),
            }),
            "mpegts",
        )
    }

    fn video(codec: &str) -> ProbeResultVideoStream {
        ProbeResultVideoStream {
            stream_index: 0,
            codec: codec.to_owned(),
            codec_type: CodecType::Video,
            dv_profile: None,
            profile: String::new(),
            height: Some(1080),
            width: Some(1920),
            frame_rate: FrameRate::parse("30000/1001"),
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            pix_fmt: String::from("yuv420p"),
            color_params: Default::default(),
            field_order: None,
            rotation: None,
        }
    }

    fn audio(codec: &str) -> ProbeResultAudioStream {
        ProbeResultAudioStream {
            stream_index: 1,
            codec: codec.to_owned(),
            channels: 2,
        }
    }

    fn decide_video(input: &ProbedInput, stream: &ProbeResultVideoStream) -> CopyDecision {
        video_copy_decision(
            &CopyPolicy::default(),
            input,
            stream,
            &VideoCopyContext::default(),
        )
    }

    #[test]
    fn allowed_codecs_copy() {
        for codec in ["h264", "hevc"] {
            assert!(decide_video(&local("matroska"), &video(codec)).is_copy());
        }
        for codec in ["aac", "ac3", "eac3", "mp3"] {
            let decision =
                audio_copy_decision(&CopyPolicy::default(), &local("mpegts"), &audio(codec));
            assert!(decision.is_copy(), "{codec}");
        }
    }

    #[test]
    fn codec_outside_policy_transcodes() {
        for codec in ["mpeg2video", "vc1", "prores"] {
            assert_eq!(
                decide_video(&local("matroska"), &video(codec)),
                CopyDecision::Transcode(vec![CopyBlocker::CodecNotAllowed(codec.to_owned())])
            );
        }
        assert_eq!(
            audio_copy_decision(&CopyPolicy::default(), &local("matroska"), &audio("flac")),
            CopyDecision::Transcode(vec![CopyBlocker::CodecNotAllowed(String::from("flac"))])
        );
    }

    #[test]
    fn mpeg2video_copies_when_listed() {
        let policy = CopyPolicy {
            formats: vec![VideoFormat::Mpeg2Video],
        };
        let decision = video_copy_decision(
            &policy,
            &local("mpegts"),
            &video("mpeg2video"),
            &VideoCopyContext::default(),
        );
        assert!(decision.is_copy());
    }

    #[test]
    fn generated_sources_transcode() {
        assert_eq!(
            decide_video(&lavfi(), &video("rawvideo")),
            CopyDecision::Transcode(vec![
                CopyBlocker::GeneratedSource,
                CopyBlocker::CodecNotAllowed(String::from("rawvideo")),
            ])
        );
        assert_eq!(
            audio_copy_decision(&CopyPolicy::default(), &lavfi(), &audio("pcm_s16le")),
            CopyDecision::Transcode(vec![
                CopyBlocker::GeneratedSource,
                CopyBlocker::CodecNotAllowed(String::from("pcm_s16le")),
            ])
        );
    }

    #[test]
    fn avi_transcodes_both_streams() {
        assert_eq!(
            decide_video(&local("avi"), &video("h264")),
            CopyDecision::Transcode(vec![CopyBlocker::ContainerWithoutPts])
        );
        assert_eq!(
            audio_copy_decision(&CopyPolicy::default(), &local("avi"), &audio("aac")),
            CopyDecision::Transcode(vec![CopyBlocker::ContainerWithoutPts])
        );
    }

    #[test]
    fn video_filters_and_dv5_transcode() {
        let stream = ProbeResultVideoStream {
            dv_profile: Some(5),
            ..video("hevc")
        };
        let context = VideoCopyContext {
            is_still_image: true,
            has_graphics: true,
            image_subtitle: true,
            burned_subtitle: true,
        };
        assert_eq!(
            video_copy_decision(
                &CopyPolicy::default(),
                &local("matroska"),
                &stream,
                &context
            ),
            CopyDecision::Transcode(vec![
                CopyBlocker::StillImage,
                CopyBlocker::DolbyVision5,
                CopyBlocker::GraphicsLayers,
                CopyBlocker::ImageSubtitle,
                CopyBlocker::BurnedSubtitle,
            ])
        );
    }

    #[test]
    fn summary_names_only_transcoded_streams() {
        assert_eq!(CopyDecisions::default().transcode_summary(), None);
        assert_eq!(
            CopyDecisions {
                video: Some(CopyDecision::Copy),
                audio: None,
            }
            .transcode_summary(),
            None
        );
        assert_eq!(
            CopyDecisions {
                video: Some(CopyDecision::Transcode(vec![
                    CopyBlocker::GraphicsLayers,
                    CopyBlocker::CodecNotAllowed(String::from("vc1")),
                ])),
                audio: Some(CopyDecision::Transcode(vec![
                    CopyBlocker::ContainerWithoutPts
                ])),
            }
            .transcode_summary()
            .as_deref(),
            Some("video (graphics layers, vc1 is not in copy_formats); audio (avi container)")
        );
    }
}
