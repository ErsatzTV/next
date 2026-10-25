mod common;

use common::*;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::pipeline::AudioFormat;
use rstest::rstest;

async fn accel() -> Option<HardwareAccel> {
    None
}

shared_tests!(accel);

#[tokio::test]
#[ignore]
async fn scale_flags() {
    let env = test_env().await.unwrap();
    let mut test_case = shared::transcode(
        "480p_h264.ts",
        "1920x1080".parse().unwrap(),
        ("h264", 8),
        AudioFormat::Aac,
    );
    test_case.params.filter_options.scale.flags = Some(String::from("lanczos"));
    let args = run_test_case(env, test_case).await;
    let filter = args.join(" ");
    assert!(
        filter.contains("scale=1440:1080:flags=lanczos,"),
        "{filter}"
    );
}

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

// Fixture tones: FL 250, FR 500, FC 1000, LFE 60, SL/BL 2000, SR/BR 4000. LFE is left out
// of the downmix lists, so whether a downmix keeps it isn't pinned.
const DOWNMIX_TONES: &[&[u32]] = &[&[250, 1000, 2000], &[500, 1000, 4000]];
const SURROUND_TONES: &[&[u32]] = &[&[250], &[500], &[1000], &[60], &[2000], &[4000]];
const STEREO_TONES: &[&[u32]] = &[&[250], &[500]];

/// Audio is never processed on the accel, so only the software suite checks it.
#[rstest]
#[tokio::test]
#[ignore]
async fn surround_audio(
    #[values(AudioFormat::Aac, AudioFormat::Ac3)] af: AudioFormat,
    #[values(2, 6)] channels: u32,
) {
    let tones = if channels == 2 {
        DOWNMIX_TONES
    } else {
        SURROUND_TONES
    };
    run_audio_test("480p_h264_ac3_51.ts", af, channels, tones).await;
}

#[rstest]
#[tokio::test]
#[ignore]
async fn source_audio_codec(
    #[values(
        ("480p_h264_eac3_51.mkv", 2, DOWNMIX_TONES),
        ("480p_h264_eac3_51.mkv", 6, SURROUND_TONES),
        ("480p_h264_dts_51.mkv", 2, DOWNMIX_TONES),
        ("480p_h264_dts_51.mkv", 6, SURROUND_TONES),
        ("480p_h264_truehd_51.mkv", 2, DOWNMIX_TONES),
        ("480p_h264_truehd_51.mkv", 6, SURROUND_TONES),
        ("480p_h264_opus_51.mkv", 2, DOWNMIX_TONES),
        ("480p_h264_opus_51.mkv", 6, SURROUND_TONES),
        ("480p_h264_flac.mkv", 2, STEREO_TONES),
        ("480p_h264_mp3.mkv", 2, STEREO_TONES),
        ("480p_h264_pcm_s24le.mkv", 2, STEREO_TONES)
    )]
    case: (&'static str, u32, &'static [&'static [u32]]),
) {
    let (src, channels, tones) = case;
    run_audio_test(src, AudioFormat::Aac, channels, tones).await;
}

async fn run_audio_test(
    src: &'static str,
    af: AudioFormat,
    channels: u32,
    tones: &'static [&'static [u32]],
) {
    let env = test_env().await.unwrap();
    let mut test_case = shared::transcode(src, "640x480".parse().unwrap(), ("h264", 8), af);
    test_case.params.audio_channels = Some(channels);
    test_case.channel_tones = Some(tones);
    run_test_case(env, test_case).await;
}

/// Text subtitles are always drawn in system memory, so the software suite covers styling.
#[tokio::test]
#[ignore]
async fn subtitle_force_style() {
    let env = test_env().await.unwrap();
    let mut test_case = shared::embedded_subtitle("1080p_h264_subtitles.mkv", 2, None, None);
    // ASS colours are &HBBGGRR
    test_case.params.subtitle_force_style =
        Some(String::from("PrimaryColour=&H000000FF,Fontsize=72,Bold=1"));
    test_case.text_color = Some([255, 0, 0]);
    run_test_case(env, test_case).await;
}

/// Convert mode runs outside the pipeline: the channel extracts the cues itself.
#[rstest]
#[case::embedded_srt("1080p_h264_subtitles.mkv", 2, "Embedded subrip", "")]
#[case::embedded_ass("1080p_h264_subtitles.mkv", 3, "Embedded ass", "")]
#[case::mov_text("720p_h264_mov_text.mp4", 2, "Embedded mov_text", "")]
#[case::sidecar_srt("subtitle.srt", 0, "Copy channel burned subtitle", "")]
#[case::sidecar_vtt("subtitle.vtt", 0, "WebVTT sidecar", "line:10% align:start")]
#[tokio::test]
#[ignore]
async fn subtitle_convert(
    #[case] src: &'static str,
    #[case] stream: u32,
    #[case] text: &str,
    #[case] settings: &str,
) {
    let env = test_env().await.unwrap();
    let path = fixture_path(src);
    let probe = probe_file(&env.ffmpeg, &env.ffprobe, &path).await;
    let input = local_input(&path, probe, std::time::Duration::from_secs(2));
    let subtitle = input
        .probe_result
        .streams
        .iter()
        .find_map(|s| match s {
            ffpipeline::probe::ProbeResultStream::Video(v) if v.stream_index == stream => Some(v),
            _ => None,
        })
        .expect("no subtitle stream at that index");
    let vtt = ffpipeline::web_vtt::convert_to_vtt(&env.ffmpeg, &input, subtitle)
        .await
        .expect("conversion failed");
    let cues = ffpipeline::web_vtt::parse_file(vtt.path())
        .await
        .expect("converted file doesn't parse");
    let cue = cues
        .iter()
        .find(|c| c.text.contains(text))
        .unwrap_or_else(|| {
            panic!(
                "no cue with {text:?}: {:?}",
                cues.iter().map(|c| &c.text).collect::<Vec<_>>()
            )
        });
    assert_eq!(cue.start, std::time::Duration::ZERO);
    assert_eq!(cue.end, std::time::Duration::from_secs(10));
    assert_eq!(cue.settings, settings);
}

/// Fixtures don't depend on the accel, so only the software suite checks them.
#[test]
#[ignore]
fn fixture_properties() {
    common::fixtures::check_all();
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
    let output = ersatztv_core::process::command(&env.ffmpeg)
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

    use ffpipeline::input::{GraphicsInput, GraphicsKind, InputSource, LocalInputSource};
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
            ersatztv_core::process::command(&env.ffmpeg)
                .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
                .args(args)
                .arg(path)
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
        kind: GraphicsKind::Canvas(ffpipeline::input::CanvasLayout::FullFrame),
        in_point: Duration::from_millis(500),
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
        ersatztv_core::process::command(&env.ffmpeg)
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
