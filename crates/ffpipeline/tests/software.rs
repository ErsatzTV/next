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

/// Generate both inputs locally so this test needs no checked-in media fixtures.
#[tokio::test]
#[ignore = "requires local ffmpeg and ffprobe"]
async fn canvas_local() {
    use std::time::Duration;

    use ffpipeline::input::{
        GraphicsInput, GraphicsKind, GraphicsLocation, InputSource, LocalInputSource,
    };
    use ffpipeline::pipeline::generate_pipeline;

    let env = test_env().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main.mkv");
    let canvas = dir.path().join("canvas.nut");
    for (path, args) in [
        (
            &main,
            vec![
                "-f",
                "lavfi",
                "-i",
                "color=blue:s=320x180:r=24",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=stereo",
                "-t",
                "4",
                "-c:v",
                "ffv1",
                "-c:a",
                "pcm_s16le",
            ],
        ),
        (
            &canvas,
            vec![
                "-f",
                "lavfi",
                "-i",
                "color=black@0:s=320x180:r=24,format=bgra,drawbox=x=0:y=0:w=80:h=80:color=red@1:t=fill:replace=1",
                "-t",
                "4",
                "-c:v",
                "ffv1",
                "-pix_fmt",
                "bgra",
                "-f",
                "nut",
            ],
        ),
    ] {
        let generated = tokio::time::timeout(
            Duration::from_secs(30),
            tokio::process::Command::new(&env.ffmpeg)
                .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
                .args(args)
                .arg(path)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .expect("fixture generation timed out")
        .unwrap();
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );
    }
    let graphics = GraphicsInput {
        layer_index: 0,
        input_source: InputSource::Local(LocalInputSource {
            path: canvas.to_string_lossy().into_owned(),
        }),
        probe_result: probe_file(&env.ffmpeg, &env.ffprobe, &canvas).await,
        stream_index: None,
        kind: GraphicsKind::Canvas,
        in_point: Duration::from_millis(500),
        location: GraphicsLocation::BottomRight,
        width_percent: Some(10.0),
        within_source_content: Some(true),
        horizontal_margin_percent: Some(5.0),
        vertical_margin_percent: Some(5.0),
        opacity_percent: Some(0.0),
        timing: None,
    };
    let probe = probe_file(&env.ffmpeg, &env.ffprobe, &main).await;
    let mut input = build_input(&main, probe, Duration::from_secs(1), Some(graphics));
    input.playout_offset = Duration::from_millis(500);
    let output = build_output(
        dir.path(),
        TestOutputParams {
            video_size: Some(FrameSize {
                width: 320,
                height: 180,
            }),
            ..Default::default()
        },
    );
    let mut pipeline = generate_pipeline(&env.ffmpeg_info, input, output).unwrap();
    pipeline.optimize();
    let args = pipeline.args();
    let canvas_index = args
        .iter()
        .position(|a| a.as_ref() == canvas.to_str().unwrap())
        .unwrap();
    assert_eq!(
        args[canvas_index - 5..canvas_index]
            .iter()
            .map(|a| a.as_ref())
            .collect::<Vec<_>>(),
        ["-ss", "1000ms", "-t", "1000ms", "-i"]
    );
    assert!(
        !args
            .iter()
            .any(|a| matches!(a.as_ref(), "-stream_loop" | "-ignore_loop" | "-framerate"))
    );
    let filter = args
        .windows(2)
        .filter(|a| a[0] == "-filter_complex")
        .map(|a| a[1].as_ref())
        .collect::<Vec<_>>()
        .join(";");
    assert!(filter.contains("overlay=x=0:y=0"), "{filter}");
    let (success, stderr) = run_ffmpeg_pipeline(&env.ffmpeg, &pipeline).await;
    assert!(success, "{stderr}");

    let segment = find_first_segment(dir.path());
    let decoded = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(&env.ffmpeg)
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(segment)
            .args([
                "-frames:v",
                "1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("frame decode timed out")
    .unwrap();
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    assert_eq!(decoded.stdout.len(), 320 * 180 * 3);
    let pixel = |x: usize, y: usize| &decoded.stdout[(y * 320 + x) * 3..(y * 320 + x) * 3 + 3];
    let red = pixel(40, 40);
    assert!(
        red[0] > 200 && red[1] < 50 && red[2] < 50,
        "canvas foreground missing: {red:?}"
    );
    let blue = pixel(200, 100);
    assert!(
        blue[0] < 50 && blue[1] < 50 && blue[2] > 200,
        "canvas background lost transparency: {blue:?}"
    );
}
