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
async fn pipeline(
    #[values("1080p_h264.ts", "720p_h264.ts", "480p_h264.ts")] src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values("h264", "hevc")] vf: VideoFormat,
    #[values("aac", "ac3")] af: AudioFormat,
) {
    run_software_test_case(TestCase {
        fixture_name: src,
        params: TestOutputParams {
            audio_format: Some(af),
            video_format: Some(vf),
            video_size: Some(res.clone()),
            ..TestOutputParams::default()
        },
        expected_video_codec: vf.to_string(),
        expected_video_size: res, // TODO: derive Copy on FrameSize
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
    if let Some(env) = test_env().await {
        run_test_case(env, test_case).await;
    }
}
