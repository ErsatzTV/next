#![cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod common;

use std::str::FromStr;

use common::*;
use ffpipeline::accel::amf::Amf;
use ffpipeline::capabilities::amf::AmfCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::input::{PeriodicClock, PeriodicTiming, WatermarkTiming};
use ffpipeline::output_settings::{LibplaceboOptions, VideoFilterOptions};
use ffpipeline::pipeline::{AudioFormat, VideoFormat};
use rstest::rstest;
use tokio::sync::OnceCell;

static AMF_ACCEL: OnceCell<Option<HardwareAccel>> = OnceCell::const_new();

async fn make_amf_accel() -> Option<&'static HardwareAccel> {
    AMF_ACCEL
        .get_or_init(|| async {
            let capabilities = AmfCapabilities::probe().ok()?;
            (capabilities.count() > 0).then(|| HardwareAccel::Amf(Amf { capabilities }))
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
        run_amf_test_case(TestCase {
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
    #[values("480i_h264.ts", "480i_h264_anamorphic.ts", "480i_mpeg2.ts")] src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_amf_test_case(TestCase {
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
        run_amf_test_case(TestCase {
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
    #[values("1080p_h264.ts", "1080p_hevc_10.ts", "720p_h264.ts", "480p_h264.ts")]
    src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_amf_test_case(TestCase {
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
        run_amf_test_case(TestCase {
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
    #[values("1080p_hevc_10_hdr.ts", "1080p_hevc_10_hdr_4x3.ts")] src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
    #[values("aac", "ac3")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_amf_test_case(TestCase {
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

#[rstest]
#[tokio::test]
#[ignore]
async fn tonemap_hdr_watermark(
    #[values("1080p_hevc_10_hdr.ts", "1080p_hevc_10_hdr_4x3.ts")] src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
    #[values("aac", "ac3")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_amf_test_case(TestCase {
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
                watermark: Some(TestWatermark::default()),
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
async fn deinterlace_motion(
    #[values("640x480", "854x480", "1920x1080")] res: FrameSize,
    #[values(VideoFormat::H264, VideoFormat::Hevc)] vf: VideoFormat,
) {
    run_amf_test_case(TestCase {
        fixture_name: "480i_h264_motion.ts",
        params: TestOutputParams {
            video_size: Some(res),
            video_format: Some(vf),
            deinterlace: true,
            ..TestOutputParams::default()
        },
        expected_video_codec: vf.to_string(),
        expected_video_size: res,
        expected_audio_codec: String::from("aac"),
    })
    .await;
}

#[rstest]
#[tokio::test]
#[ignore]
async fn canvas(
    #[values(("ffv1", "bgra"), ("ffv1", "yuva420p"), ("ffv1", "yuva444p"), ("png", "rgba"))]
    source: (&'static str, &'static str),
) {
    if let Some(env) = test_env().await {
        if !env.ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Amf) {
            panic!("amf not available");
        }

        let Some(accel) = make_amf_accel().await else {
            panic!("amf accel failed to probe any capabilities");
        };

        run_canvas_test(env, Some(accel.clone()), source.0, source.1).await;
    }
}

async fn run_amf_test_case(mut test_case: TestCase) {
    if let Some(env) = test_env().await {
        if !env.ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Amf) {
            panic!("amf not available");
        }

        let Some(accel) = make_amf_accel().await else {
            panic!("amf accel failed to probe any capabilities");
        };

        test_case.params.accel = Some(accel.clone());
        run_test_case(env, test_case).await;
    }
}
