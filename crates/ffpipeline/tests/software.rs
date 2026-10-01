mod common;

use common::*;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use rstest::rstest;

async fn accel() -> Option<HardwareAccel> {
    None
}

shared_tests!(accel);

#[tokio::test]
#[ignore]
async fn copy_input_start() {
    common::copy_seek::run_input_start_test().await;
}

/// Copy doesn't use the accel, so only the software suite runs this.
#[rstest]
#[tokio::test]
#[ignore]
async fn copy_seek(
    #[values(
        ("long_gop_h264.mp4", false),
        ("long_gop_h264.mkv", false),
        ("long_gop_h264.ts", false),
        ("long_gop_h264_open.ts", true),
        ("long_gop_hevc_open.ts", true)
    )]
    fixture: (&'static str, bool),
    #[values(6.5, 9.9)] join: f64,
) {
    let (fixture_name, open_gop) = fixture;
    common::copy_seek::run_copy_seek_test(common::copy_seek::CopySeekCase {
        fixture_name,
        join: std::time::Duration::from_secs_f64(join),
        open_gop,
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
