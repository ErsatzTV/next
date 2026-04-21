#![cfg(target_os = "macos")]
mod common;

use common::*;
use ffpipeline::accel::video_toolbox::VideoToolbox;
use ffpipeline::capabilities::videotoolbox::VideoToolboxCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::pipeline::{AudioFormat, VideoFormat};
use rstest::rstest;

fn make_videotoolbox_accel() -> Option<HardwareAccel> {
    let capabilities = VideoToolboxCapabilities::probe().ok()?;
    Some(HardwareAccel::VideoToolbox(VideoToolbox { capabilities }))
}

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
    run_videotoolbox_test_case(TestCase {
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

async fn run_videotoolbox_test_case(mut test_case: TestCase) {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;
    if !ffmpeg_info.has_hw_accel(&KnownHardwareAccel::VideoToolbox) {
        eprintln!("skip: videotoolbox not available");
        return;
    }

    let Some(accel) = make_videotoolbox_accel() else {
        eprintln!("skip: videotoolbox accel failed to probe");
        return;
    };

    test_case.params.accel = Some(accel);
    run_test_case(&ffmpeg, &ffprobe, &ffmpeg_info, test_case).await;
}
