#![cfg(target_os = "linux")]
mod common;

use std::path::PathBuf;
use std::str::FromStr;

use common::*;
use ffpipeline::accel::vaapi::{Vaapi, VaapiDriver};
use ffpipeline::capabilities::vaapi::VaapiCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::pipeline::{AudioFormat, VideoFormat};
use rstest::rstest;
use tokio::sync::OnceCell;

static VAAPI_ACCEL: OnceCell<Option<HardwareAccel>> = OnceCell::const_new();

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

async fn make_vaapi_accel() -> Option<&'static HardwareAccel> {
    VAAPI_ACCEL
        .get_or_init(|| async {
            let (device, driver, capabilities) = probe_vaapi()?;
            Some(HardwareAccel::Vaapi(Vaapi {
                device,
                driver,
                capabilities,
                needs_opencl_device: false,
            }))
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
        "1080p_hevc_10.ts",
        "720p_hevc_10.ts",
        "480p_hevc_10.ts"
    )]
    src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
    #[values("aac", "ac3")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_vaapi_test_case(TestCase {
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

async fn run_vaapi_test_case(mut test_case: TestCase) {
    if let Some(env) = test_env().await {
        if !env.ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Vaapi) {
            panic!("vaapi not available in ffmpeg");
        };

        let Some(accel) = make_vaapi_accel().await else {
            panic!("no usable VAAPI device/driver found");
        };

        test_case.params.accel = Some(accel.clone());
        run_test_case(env, test_case).await;
    }
}
