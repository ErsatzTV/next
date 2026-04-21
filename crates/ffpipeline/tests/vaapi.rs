#![cfg(target_os = "linux")]
mod common;

use std::path::PathBuf;

use common::*;
use ffpipeline::accel::vaapi::{Vaapi, VaapiDriver};
use ffpipeline::capabilities::vaapi::VaapiCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::pipeline::{AudioFormat, VideoFormat};
use rstest::rstest;

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
        if let Ok(caps) = VaapiCapabilities::probe(device_str, &driver.to_string())
            && caps.count() > 0
        {
            return Some((device_str.to_owned(), driver, caps));
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

#[rstest]
#[tokio::test]
#[ignore]
async fn transcode_matrix(
    #[values(AudioFormat::Aac, AudioFormat::Ac3)] af: AudioFormat,
    #[values(VideoFormat::H264, VideoFormat::Hevc)] vf: VideoFormat,
    #[values(FrameSize { width: 1920, height: 1080 }, FrameSize { width: 1280, height: 720 })]
    target_size: FrameSize,
    #[values("1080p_h264.ts", "720p_h264.ts", "480p_h264.ts")] fixture_name: &'static str,
) {
    run_vaapi_test_case(TestCase {
        fixture_name,
        params: TestOutputParams {
            audio_format: Some(af),
            video_format: Some(vf),
            video_size: Some(target_size.clone()),
            ..TestOutputParams::default()
        },
        expected_video_codec: vf.to_string(),
        expected_video_size: target_size, // TODO: derive Copy on FrameSize
        expected_audio_codec: af.to_string(),
    })
    .await;
}

async fn run_vaapi_test_case(mut test_case: TestCase) {
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

    test_case.params.accel = Some(accel);
    run_test_case(&ffmpeg, &ffprobe, &ffmpeg_info, test_case).await;
}
