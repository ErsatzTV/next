#![cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
mod common;

use std::str::FromStr;

use common::*;
use ffpipeline::accel::qsv::Qsv;
use ffpipeline::capabilities::qsv::QsvCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::input::{PeriodicClock, PeriodicTiming, WatermarkTiming};
use ffpipeline::output_settings::{LibplaceboOptions, VideoFilterOptions};
use ffpipeline::pipeline::{AudioFormat, VideoFormat};
use rstest::rstest;
use tokio::sync::OnceCell;

static QSV_ACCEL: OnceCell<Option<HardwareAccel>> = OnceCell::const_new();

async fn make_qsv_accel() -> Option<&'static HardwareAccel> {
    QSV_ACCEL
        .get_or_init(|| async {
            let capabilities = QsvCapabilities::probe().ok()?;
            (capabilities.count() > 0).then(|| HardwareAccel::Qsv(Qsv { capabilities }))
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
        run_qsv_test_case(TestCase {
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
async fn watermark(
    #[values("1080p_h264.ts", "1080p_hevc_10.ts", "720p_h264.ts", "480p_h264.ts")]
    src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_qsv_test_case(TestCase {
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

/// Exercises fades over a still image, which need the looped (repeated) frames to carry timestamps.
#[rstest]
#[tokio::test]
#[ignore]
async fn watermark_periodic(
    #[values("1080p_h264.ts", "720p_h264.ts")] src: &'static str,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let res = FrameSize::from_str("1920x1080").unwrap();
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_qsv_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                watermark: Some(TestWatermark {
                    timing: Some(WatermarkTiming::Periodic(PeriodicTiming {
                        clock: PeriodicClock::Content,
                        frequency_ms: 2000,
                        phase_offset_ms: None,
                        disable_after_ms: None,
                        fade_ms: Some(200),
                        hold_ms: 400,
                    })),
                    ..TestWatermark::default()
                }),
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
async fn tonemap_hdr(
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
    #[values("aac", "ac3")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_qsv_test_case(TestCase {
            fixture_name: "1080p_hevc_10_hdr.ts",
            params: TestOutputParams {
                audio_format: Some(af),
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                filter_options: VideoFilterOptions {
                    libplacebo: LibplaceboOptions {
                        tonemapping: Some("hable".to_string()),
                    },
                    ..VideoFilterOptions::default()
                },
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
async fn tonemap_dv(
    #[values(
        "1080p_hevc_10_dv5.mp4",
        "1080p_hevc_10_dv7.mp4",
        "1080p_hevc_10_dv81.mp4",
        "1080p_hevc_10_dv82.mp4",
        "1080p_hevc_10_dv84.mp4"
    )]
    src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
    #[values("aac", "ac3")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_qsv_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                audio_format: Some(af),
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                filter_options: VideoFilterOptions {
                    libplacebo: LibplaceboOptions {
                        tonemapping: Some("hable".to_string()),
                    },
                    ..VideoFilterOptions::default()
                },
                ..TestOutputParams::default()
            },
            expected_video_codec: vf.to_string(),
            expected_video_size: res,
            expected_audio_codec: af.to_string(),
        })
        .await;
    }
}

async fn run_qsv_test_case(mut test_case: TestCase) {
    if let Some(env) = test_env().await {
        if !env.ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Qsv) {
            panic!("qsv not available");
        }

        let Some(accel) = make_qsv_accel().await else {
            panic!("qsv accel failed to probe any capabilities");
        };

        test_case.params.accel = Some(accel.clone());
        run_test_case(env, test_case).await;
    }
}
