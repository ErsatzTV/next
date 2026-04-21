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
async fn pipeline(
    #[values("1080p_h264.ts", "720p_h264.ts", "480p_h264.ts")] src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values("h264", "hevc")] vf: VideoFormat,
    #[values("aac", "ac3")] af: AudioFormat,
) {
    run_videotoolbox_test_case(TestCase {
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

async fn run_videotoolbox_test_case(mut test_case: TestCase) {
    if let Some(env) = test_env().await {
        if !env
            .ffmpeg_info
            .has_hw_accel(&KnownHardwareAccel::VideoToolbox)
        {
            panic!("videotoolbox not available");
        }

        let Some(accel) = make_videotoolbox_accel().await else {
            panic!("videotoolbox accel failed to probe");
        };

        test_case.params.accel = Some(accel.clone());
        run_test_case(env, test_case).await;
    }
}
