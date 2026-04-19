#![cfg(target_os = "linux")]
mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::*;
use ffpipeline::accel::vaapi::{Vaapi, VaapiDriver};
use ffpipeline::capabilities::vaapi::VaapiCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::pipeline::{generate_pipeline, VideoFormat};

fn find_vaapi_device() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("ETV_TEST_VAAPI_DEVICE") {
        return Some(PathBuf::from(path));
    }
    let path = PathBuf::from("/dev/dri/renderD128");
    path.exists().then_some(path)
}

fn find_vaapi_driver() -> Option<VaapiDriver> {
    if let Ok(name) = std::env::var("ETV_TEST_VAAPI_DRIVER") {
        return match name.as_str() {
            "ihd" | "iHD" => Some(VaapiDriver::Ihd),
            "i965" => Some(VaapiDriver::I965),
            "radeonsi" => Some(VaapiDriver::RadeonSI),
            _ => None,
        };
    }
    None
}

fn probe_vaapi() -> Option<(String, VaapiDriver, VaapiCapabilities)> {
    let device = find_vaapi_device()?;
    let device_str = device.to_str()?;

    if let Some(driver) = find_vaapi_driver() {
        let caps = VaapiCapabilities::probe(device_str, &driver.to_string()).ok()?;
        return Some((device_str.to_owned(), driver, caps));
    }

    for driver in [VaapiDriver::Ihd, VaapiDriver::I965, VaapiDriver::RadeonSI] {
        if let Ok(caps) = VaapiCapabilities::probe(device_str, &driver.to_string()) {
            if caps.count() > 0 {
                return Some((device_str.to_owned(), driver, caps));
            }
        }
    }

    None
}

fn make_vaapi_accel() -> Option<HardwareAccel> {
    let (device, driver, capabilities) = probe_vaapi()?;
    Some(HardwareAccel::Vaapi(Vaapi {
        device,
        driver,
        capabilities,
    }))
}

#[tokio::test]
#[ignore]
async fn vaapi_h264() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;
    if !ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Vaapi) {
        eprintln!("skip: vaapi not available in ffmpeg");
        return;
    }

    let Some(accel) = make_vaapi_accel() else {
        eprintln!("skip: no usable VAAPI device/driver found");
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
async fn vaapi_hevc() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;
    if !ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Vaapi) {
        eprintln!("skip: vaapi not available in ffmpeg");
        return;
    };

    let Some(accel) = make_vaapi_accel() else {
        eprintln!("skip: no usable VAAPI device/driver found");
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

#[tokio::test]
#[ignore]
async fn vaapi_scale_1080p_to_720p() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;
    if !ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Vaapi) {
        eprintln!("skip: vaapi not available in ffmpeg");
        return;
    };

    let Some(accel) = make_vaapi_accel() else {
        eprintln!("skip: no usable VAAPI device/driver found");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("1080p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(
        dir.path(),
        TestOutputParams {
            video_size: Some(FrameSize {
                width: 1280,
                height: 720,
            }),
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
    assert_video(&output_probe, "h264", 1280, 720);
    assert_audio(&output_probe, "aac");
}

#[tokio::test]
#[ignore]
async fn vaapi_pad_4x3_to_16x9() {
    let Some((ffmpeg, ffprobe)) = find_binaries() else {
        eprintln!("skip: ffmpeg/ffprobe not found");
        return;
    };

    let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;
    if !ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Vaapi) {
        eprintln!("skip: vaapi not available in ffmpeg");
        return;
    };

    let Some(accel) = make_vaapi_accel() else {
        eprintln!("skip: no usable VAAPI device/driver found");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path("480p_h264.ts");
    let probe = probe_file(&ffmpeg, &ffprobe, &source).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(
        dir.path(),
        TestOutputParams {
            video_size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
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
