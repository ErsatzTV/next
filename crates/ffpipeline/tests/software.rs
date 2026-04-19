mod common;

use std::time::Duration;

use common::*;
use ffpipeline::frame_rate::FrameRate;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::output_settings::AudioLoudnessSettings;
use ffpipeline::pipeline::{AudioFormat, VideoFormat, generate_pipeline};

#[tokio::test]
#[ignore]
async fn h264_transcode() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("1080p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;
    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(dir.path(), TestOutputParams::default());

    let mut pipeline = generate_pipeline(&ffmpeg_info, input, output).unwrap();
    pipeline.optimize();

    let (success, stderr) = run_ffmpeg_pipeline(&ffmpeg, &pipeline).await;
    assert!(success, "ffmpeg failed:\n{stderr}");

    let segment = find_first_segment(dir.path());
    let output_probe = probe_file(&ffmpeg, &ffprobe, &segment).await;
    assert_video(&output_probe, "h264", 1920, 1080);
    assert_audio(&output_probe, "aac");
}

#[tokio::test]
#[ignore]
async fn hevc_transcode() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("1080p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;
    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(
        dir.path(),
        TestOutputParams {
            video_format: Some(VideoFormat::Hevc),
            ..TestOutputParams::default()
        },
    );

    let mut pipeline = generate_pipeline(&ffmpeg_info, input, output).unwrap();
    pipeline.optimize();

    let (success, stderr) = run_ffmpeg_pipeline(&ffmpeg, &pipeline).await;
    assert!(success, "ffmpeg failed:\n{stderr}");

    let segment = find_first_segment(dir.path());
    let output_probe = probe_file(&ffmpeg, &ffprobe, &segment).await;
    assert_video(&output_probe, "hevc", 1920, 1080);
    assert_audio(&output_probe, "aac");
}

#[tokio::test]
#[ignore]
async fn codec_copy() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("720p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;
    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(
        dir.path(),
        TestOutputParams {
            video_format: None,
            audio_format: None,
            video_bitrate: None,
            video_buffer: None,
            ..TestOutputParams::default()
        },
    );

    let mut pipeline = generate_pipeline(&ffmpeg_info, input, output).unwrap();
    pipeline.optimize();

    let (success, stderr) = run_ffmpeg_pipeline(&ffmpeg, &pipeline).await;
    assert!(success, "ffmpeg failed:\n{stderr}");

    let segment = find_first_segment(dir.path());
    let output_probe = probe_file(&ffmpeg, &ffprobe, &segment).await;
    assert_video(&output_probe, "h264", 1280, 720);
    assert_audio(&output_probe, "aac");
}

#[tokio::test]
#[ignore]
async fn scale_1080p_to_720p() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("1080p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;
    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(
        dir.path(),
        TestOutputParams {
            video_size: Some(FrameSize {
                width: 1280,
                height: 720,
            }),
            ..TestOutputParams::default()
        },
    );

    let mut pipeline = generate_pipeline(&ffmpeg_info, input, output).unwrap();
    pipeline.optimize();

    let (success, stderr) = run_ffmpeg_pipeline(&ffmpeg, &pipeline).await;
    assert!(success, "ffmpeg failed:\n{stderr}");

    let segment = find_first_segment(dir.path());
    let output_probe = probe_file(&ffmpeg, &ffprobe, &segment).await;
    assert_video(&output_probe, "h264", 1280, 720);
    assert_audio(&output_probe, "aac");
}

#[tokio::test]
#[ignore]
async fn pad_4x3_to_16x9() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("480p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;
    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(
        dir.path(),
        TestOutputParams {
            video_size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
            ..TestOutputParams::default()
        },
    );

    let mut pipeline = generate_pipeline(&ffmpeg_info, input, output).unwrap();
    pipeline.optimize();

    let (success, stderr) = run_ffmpeg_pipeline(&ffmpeg, &pipeline).await;
    assert!(success, "ffmpeg failed:\n{stderr}");

    let segment = find_first_segment(dir.path());
    let output_probe = probe_file(&ffmpeg, &ffprobe, &segment).await;
    assert_video(&output_probe, "h264", 1920, 1080);
    assert_audio(&output_probe, "aac");
}

#[tokio::test]
#[ignore]
async fn ac3_audio() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("1080p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;
    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(
        dir.path(),
        TestOutputParams {
            audio_format: Some(AudioFormat::Ac3),
            ..TestOutputParams::default()
        },
    );

    let mut pipeline = generate_pipeline(&ffmpeg_info, input, output).unwrap();
    pipeline.optimize();

    let (success, stderr) = run_ffmpeg_pipeline(&ffmpeg, &pipeline).await;
    assert!(success, "ffmpeg failed:\n{stderr}");

    let segment = find_first_segment(dir.path());
    let output_probe = probe_file(&ffmpeg, &ffprobe, &segment).await;
    assert_video(&output_probe, "h264", 1920, 1080);
    assert_audio(&output_probe, "ac3");
}

#[tokio::test]
#[ignore]
async fn loudness_normalization() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("1080p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;
    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(
        dir.path(),
        TestOutputParams {
            loudness: Some(AudioLoudnessSettings::default()),
            ..TestOutputParams::default()
        },
    );

    let mut pipeline = generate_pipeline(&ffmpeg_info, input, output).unwrap();
    pipeline.optimize();

    let (success, stderr) = run_ffmpeg_pipeline(&ffmpeg, &pipeline).await;
    assert!(success, "ffmpeg failed:\n{stderr}");

    let segment = find_first_segment(dir.path());
    let output_probe = probe_file(&ffmpeg, &ffprobe, &segment).await;
    assert_video(&output_probe, "h264", 1920, 1080);
    assert_audio(&output_probe, "aac");
}

#[tokio::test]
#[ignore]
async fn custom_frame_rate() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("1080p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;
    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(
        dir.path(),
        TestOutputParams {
            frame_rate: Some(FrameRate::parse("24")),
            ..TestOutputParams::default()
        },
    );

    let mut pipeline = generate_pipeline(&ffmpeg_info, input, output).unwrap();
    pipeline.optimize();

    let (success, stderr) = run_ffmpeg_pipeline(&ffmpeg, &pipeline).await;
    assert!(success, "ffmpeg failed:\n{stderr}");

    let segment = find_first_segment(dir.path());
    let output_probe = probe_file(&ffmpeg, &ffprobe, &segment).await;
    assert_video(&output_probe, "h264", 1920, 1080);
    assert_audio(&output_probe, "aac");
}
