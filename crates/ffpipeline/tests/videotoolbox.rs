#![cfg(target_os = "macos")]
mod common;

use std::time::Duration;

use common::*;
use ffpipeline::accel::video_toolbox::VideoToolbox;
use ffpipeline::capabilities::videotoolbox::VideoToolboxCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::pipeline::{VideoFormat, generate_pipeline};

fn make_videotoolbox_accel() -> Option<HardwareAccel> {
    let capabilities = VideoToolboxCapabilities::probe().ok()?;
    Some(HardwareAccel::VideoToolbox(VideoToolbox { capabilities }))
}

#[tokio::test]
#[ignore]
async fn videotoolbox_h264() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;
    if !ffmpeg_info.has_hw_accel(&KnownHardwareAccel::VideoToolbox) {
        eprintln!("skip: videotoolbox not available");
        return;
    }

    let Some(accel) = make_videotoolbox_accel() else {
        eprintln!("skip: videotoolbox accel failed to probe");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("1080p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(
        dir.path(),
        TestOutputParams {
            accel: Some(accel),
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
#[cfg(target_os = "macos")]
async fn videotoolbox_hevc() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;
    if !ffmpeg_info.has_hw_accel(&KnownHardwareAccel::VideoToolbox) {
        eprintln!("skip: videotoolbox not available");
        return;
    }

    let Some(accel) = make_videotoolbox_accel() else {
        eprintln!("skip: videotoolbox accel failed to probe");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("1080p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(
        dir.path(),
        TestOutputParams {
            video_format: Some(VideoFormat::Hevc),
            accel: Some(accel),
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
