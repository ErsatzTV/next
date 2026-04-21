mod common;

use common::*;
use ffpipeline::frame_rate::FrameRate;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::output_settings::AudioLoudnessSettings;
use ffpipeline::pipeline::{AudioFormat, VideoFormat};
use rstest::rstest;

#[rstest]
#[tokio::test]
#[ignore]
async fn transcode_matrix(
    #[values(AudioFormat::Aac, AudioFormat::Ac3)] af: AudioFormat,
    #[values(VideoFormat::H264, VideoFormat::Hevc)] vf: VideoFormat,
    #[values(FrameSize { width: 1920, height: 1080 }, FrameSize { width: 1280, height: 720 })]
    target_size: FrameSize,
    #[values("1080p_h264.ts", "720p_h264.ts", "480p_h264.ts")] fixture_name: &'static str,
) {
    run_software_test_case(TestCase {
        fixture_name,
        params: TestOutputParams {
            audio_format: Some(af),
            video_format: Some(vf),
            video_size: Some(target_size.clone()),
            ..TestOutputParams::default()
        },
        expected_video_codec: vf.to_string(),
        expected_video_size: target_size, // TODO: derive Copy on FrameSize
        expected_audio_codec: af.to_string(),
    })
    .await;
}

#[tokio::test]
#[ignore]
async fn codec_copy() {
    run_software_test_case(TestCase {
        fixture_name: "720p_h264.ts",
        params: TestOutputParams {
            video_format: None,
            audio_format: None,
            video_bitrate: None,
            video_buffer: None,
            ..TestOutputParams::default()
        },
        expected_video_codec: String::from("h264"),
        expected_video_size: FrameSize {
            width: 1280,
            height: 720,
        },
        expected_audio_codec: String::from("aac"),
    })
    .await;
}

#[tokio::test]
#[ignore]
async fn loudness_normalization() {
    run_software_test_case(TestCase {
        fixture_name: "1080p_h264.ts",
        params: TestOutputParams {
            loudness: Some(AudioLoudnessSettings::default()),
            ..TestOutputParams::default()
        },
        expected_video_codec: String::from("h264"),
        expected_video_size: FrameSize {
            width: 1920,
            height: 1080,
        },
        expected_audio_codec: String::from("aac"),
    })
    .await;
}

#[tokio::test]
#[ignore]
async fn custom_frame_rate() {
    run_software_test_case(TestCase {
        fixture_name: "1080p_h264.ts",
        params: TestOutputParams {
            frame_rate: Some(FrameRate::parse("24")),
            ..TestOutputParams::default()
        },
        expected_video_codec: String::from("h264"),
        expected_video_size: FrameSize {
            width: 1920,
            height: 1080,
        },
        expected_audio_codec: String::from("aac"),
    })
    .await;
}

async fn run_software_test_case(test_case: TestCase) {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };
    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;
    run_test_case(&ffmpeg, &ffprobe, &ffmpeg_info, test_case).await;
}
