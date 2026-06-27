mod common;

use std::str::FromStr;

use common::*;
use ffpipeline::accel::rkmpp::Rkmpp;
use ffpipeline::capabilities::rkmpp::RkmppCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::pipeline::{AudioFormat, VideoFormat};
use rstest::rstest;
use tokio::sync::OnceCell;

static RKMPP_ACCEL: OnceCell<Option<HardwareAccel>> = OnceCell::const_new();

async fn make_rkmpp_accel() -> Option<&'static HardwareAccel> {
    RKMPP_ACCEL
        .get_or_init(|| async {
            let capabilities = RkmppCapabilities::probe().ok()?;
            Some(HardwareAccel::Rkmpp(Rkmpp { capabilities }))
        })
        .await
        .as_ref()
}

#[rstest]
#[tokio::test]
#[ignore]
async fn pipeline(
    #[values(
        "1080p_h264.ts",
        "720p_h264.ts",
        "480p_h264.ts",
        "1080p_h264_10.ts",
        "720p_h264_10.ts",
        "480p_h264_10.ts",
        "1080p_hevc_10.ts",
        "720p_hevc_10.ts",
        "480p_hevc_10.ts",
        "480p_h264_anamorphic.ts"
    )]
    src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
    #[values("aac", "ac3")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_rkmpp_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                audio_format: Some(af),
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                ..TestOutputParams::default()
            },
            expected_video_codec: vf.to_string(),
            expected_video_size: res,
            expected_audio_codec: af.to_string(),
        })
        .await;
    }
}

async fn run_rkmpp_test_case(mut test_case: TestCase) {
    if let Some(env) = test_env().await {
        if !env.ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Rkmpp) {
            panic!("rkmpp not available");
        }

        let Some(accel) = make_rkmpp_accel().await else {
            panic!("rkmpp accel failed to probe");
        };

        test_case.params.accel = Some(accel.clone());
        run_test_case(env, test_case).await;
    }
}
