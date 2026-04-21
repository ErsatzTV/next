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
use tokio::sync::OnceCell;

static VIDEOTOOLBOX_ACCEL: OnceCell<Option<HardwareAccel>> = OnceCell::const_new();

async fn make_videotoolbox_accel() -> Option<&'static HardwareAccel> {
    VIDEOTOOLBOX_ACCEL
        .get_or_init(|| async {
            let capabilities = VideoToolboxCapabilities::probe().ok()?;
            Some(HardwareAccel::VideoToolbox(VideoToolbox { capabilities }))
        })
        .await
        .as_ref()
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
    if let Some(env) = test_env().await {
        if !env
            .ffmpeg_info
            .has_hw_accel(&KnownHardwareAccel::VideoToolbox)
        {
            eprintln!("skip: videotoolbox not available");
            return;
        }

        let Some(accel) = make_videotoolbox_accel().await else {
            eprintln!("skip: videotoolbox accel failed to probe");
            return;
        };

        test_case.params.accel = Some(accel.clone());
        run_test_case(env, test_case).await;
    }
}
