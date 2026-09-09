mod common;

use std::str::FromStr;

use common::*;
use ffpipeline::frame_rate::FrameRate;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::input::{PeriodicClock, PeriodicTiming, WatermarkTiming};
use ffpipeline::output_settings::AudioLoudnessSettings;
use ffpipeline::pipeline::{AudioFormat, VideoFormat};
use rstest::rstest;

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
        run_software_test_case(TestCase {
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

#[rstest]
#[tokio::test]
#[ignore]
async fn tonemap_hdr(
    #[values("1080p_hevc_10_hdr.ts", "1080p_hevc_10_hdr_4x3.ts")] src: &'static str,
    #[values("2560x1440", "1920x1080", "1280x720")] res: FrameSize,
    #[values(("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
    #[values("aac", "ac3")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_software_test_case(TestCase {
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
        run_software_test_case(TestCase {
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
    #[values("480p_h264_interlaced.ts", "480p_h264_anamorphic_interlaced.ts")] src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_software_test_case(TestCase {
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

#[rstest]
#[tokio::test]
#[ignore]
async fn watermark(
    #[values(
        "1080p_hevc_10.ts",
        "1080p_h264.ts",
        "720p_h264.ts",
        "480p_h264_anamorphic.ts",
        "480p_h264_sps_change.ts"
    )]
    src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_software_test_case(TestCase {
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

/// Exercises the `-ignore_loop 0` input branch instead of the single-frame still-image branch.
#[rstest]
#[tokio::test]
#[ignore]
async fn watermark_animated(
    #[values("1080p_h264.ts", "480p_h264_anamorphic.ts")] src: &'static str,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let res = FrameSize::from_str("1920x1080").unwrap();
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_software_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                watermark: Some(TestWatermark {
                    fixture_name: "watermark.gif",
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

/// Exercises fades over a still image, which need the looped (repeated) frames to carry timestamps.
#[rstest]
#[tokio::test]
#[ignore]
async fn watermark_periodic(
    #[values("1080p_h264.ts", "480p_h264_anamorphic.ts")] src: &'static str,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let res = FrameSize::from_str("1920x1080").unwrap();
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_software_test_case(TestCase {
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

async fn run_software_test_case(test_case: TestCase) {
    if let Some(env) = test_env().await {
        run_test_case(env, test_case).await;
    }
}

#[rstest]
#[tokio::test]
#[ignore]
async fn deinterlace_motion(
    #[values("640x480", "854x480", "1920x1080")] res: FrameSize,
    #[values(VideoFormat::H264, VideoFormat::Hevc)] vf: VideoFormat,
) {
    run_software_test_case(TestCase {
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

// Negative control: progressive encoder metadata does not prove deinterlacing.
#[rstest]
#[tokio::test]
#[ignore]
async fn motion_check_rejects_missing_deinterlace(
    #[values("640x480", "854x480", "1920x1080")] res: FrameSize,
) {
    let env = test_env().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("combed.ts");
    let filter = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease:force_divisible_by=2,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=1",
        res.width, res.height, res.width, res.height,
    );
    let output = tokio::process::Command::new(&env.ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(fixture_path("480i_h264_motion.ts"))
        .args(["-t", "1", "-an", "-vf", &filter, "-c:v", "libx264"])
        .arg(&output_path)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let probe = probe_file(&env.ffmpeg, &env.ffprobe, &output_path).await;
    let video = probe
        .streams
        .iter()
        .find_map(|s| match s {
            ffpipeline::probe::ProbeResultStream::Video(v) => Some(v),
            _ => None,
        })
        .unwrap();
    assert_eq!(video.field_order.as_deref(), Some("progressive"));
    let score = motion_combing_score(&env.ffmpeg, &output_path, res).await;
    assert!(
        score >= 1.0,
        "negative control failed to detect combing: {score}"
    );
}
