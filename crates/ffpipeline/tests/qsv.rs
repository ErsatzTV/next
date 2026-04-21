#![cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
mod common;

use common::*;
use ffpipeline::accel::qsv::Qsv;
use ffpipeline::capabilities::qsv::QsvCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::pipeline::{AudioFormat, VideoFormat};
use rstest::rstest;

fn make_qsv_accel() -> Option<HardwareAccel> {
    let capabilities = QsvCapabilities::probe().ok()?;
    Some(HardwareAccel::Qsv(Qsv { capabilities }))
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
    run_qsv_test_case(TestCase {
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

async fn run_qsv_test_case(mut test_case: TestCase) {
    if let Some(env) = test_env().await {
        if !env.ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Qsv) {
            eprintln!("skip: qsv not available");
            return;
        }

        let Some(accel) = make_qsv_accel() else {
            eprintln!("skip: qsv accel failed to probe");
            return;
        };

        test_case.params.accel = Some(accel);
        run_test_case(env, test_case).await;
    }
}
