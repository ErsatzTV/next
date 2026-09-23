#![cfg(target_os = "macos")]
mod common;

use std::str::FromStr;

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
            (capabilities.count() > 0)
                .then(|| HardwareAccel::VideoToolbox(VideoToolbox { capabilities }))
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
        "480p_h264_anamorphic.ts",
        "480p_h264_sps_change.ts"
    )]
    src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
    #[values("aac", "ac3")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_videotoolbox_test_case(TestCase {
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

#[rstest]
#[tokio::test]
#[ignore]
async fn deinterlace(
    #[values("480i_h264.ts", "480i_h264_anamorphic.ts")] src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_videotoolbox_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                deinterlace: true,
                ..TestOutputParams::default()
            },
            expected_video_codec: vf.to_string(),
            expected_video_size: res,
            expected_audio_codec: String::from("aac"),
        })
        .await;
    }
}

/// 16:9 interlaced source transcoded with deinterlacing off: no pad is needed, so
/// hardware pipelines keep decoded frames on the device all the way to the encoder.
#[rstest]
#[tokio::test]
#[ignore]
async fn interlaced_no_deinterlace(
    #[values("1080i_h264.ts")] src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_videotoolbox_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                deinterlace: false,
                ..TestOutputParams::default()
            },
            expected_video_codec: vf.to_string(),
            expected_video_size: res,
            expected_audio_codec: String::from("aac"),
        })
        .await;
    }
}

#[rstest]
#[tokio::test]
#[ignore]
async fn watermark(
    #[values("1080p_h264.ts", "720p_h264.ts", "480p_h264.ts")] src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_videotoolbox_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                watermark: Some(TestWatermark::default()),
                ..TestOutputParams::default()
            },
            expected_video_codec: vf.to_string(),
            expected_video_size: res,
            expected_audio_codec: AudioFormat::Aac.to_string(),
        })
        .await;
    }
}

#[rstest]
#[tokio::test]
#[ignore]
async fn canvas(
    #[values(("ffv1", "bgra"), ("ffv1", "yuva420p"), ("ffv1", "yuva444p"), ("png", "rgba"))]
    source: (&'static str, &'static str),
) {
    if let Some(env) = test_env().await {
        if !env
            .ffmpeg_info
            .has_hw_accel(&KnownHardwareAccel::VideoToolbox)
        {
            panic!("videotoolbox not available");
        }

        let Some(accel) = make_videotoolbox_accel().await else {
            panic!("videotoolbox accel failed to probe any capabilities");
        };

        run_canvas_test(env, Some(accel.clone()), source.0, source.1).await;
    }
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
            panic!("videotoolbox accel failed to probe any capabilities");
        };

        test_case.params.accel = Some(accel.clone());
        run_test_case(env, test_case).await;
    }
}
