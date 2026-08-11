#![cfg(target_os = "linux")]
mod common;

use std::path::PathBuf;
use std::str::FromStr;

use common::*;
use ffpipeline::accel::vaapi::{Vaapi, VaapiDriver};
use ffpipeline::capabilities::opencl::OpenCLCapabilities;
use ffpipeline::capabilities::vaapi::VaapiCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::output_settings::{TonemapOpenclOptions, VideoFilterOptions};
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

fn probe_vaapi() -> Option<(String, VaapiDriver, VaapiCapabilities, OpenCLCapabilities)> {
    let device = find_vaapi_device()?;
    let device_str = device.to_str()?;

    if let Some(driver) = find_vaapi_driver() {
        let caps = VaapiCapabilities::probe(device_str, Some(&driver.to_string())).ok()?;
        let opencl_caps = OpenCLCapabilities::probe().unwrap_or_default();
        return Some((device_str.to_owned(), driver, caps, opencl_caps));
    }

    for driver in [VaapiDriver::Ihd, VaapiDriver::I965, VaapiDriver::RadeonSI] {
        if let Ok(caps) = VaapiCapabilities::probe(device_str, Some(&driver.to_string()))
            && caps.count() > 0
        {
            let opencl_caps = OpenCLCapabilities::probe().unwrap_or_default();
            return Some((device_str.to_owned(), driver, caps, opencl_caps));
        }
    }

    None
}

async fn make_vaapi_accel() -> Option<&'static HardwareAccel> {
    VAAPI_ACCEL
        .get_or_init(|| async {
            let (device, driver, capabilities, opencl_capabilities) = probe_vaapi()?;
            Some(HardwareAccel::Vaapi(Vaapi {
                device,
                driver,
                capabilities,
                opencl_capabilities,
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

#[rstest]
#[tokio::test]
#[ignore]
async fn tonemap_hdr(
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
    #[values("aac", "ac3")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_vaapi_test_case(TestCase {
            fixture_name: "1080p_hevc_10_hdr.ts",
            params: TestOutputParams {
                audio_format: Some(af),
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                filter_options: VideoFilterOptions {
                    tonemap_opencl: TonemapOpenclOptions {
                        tonemap: Some("hable".to_string()),
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
        run_vaapi_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                audio_format: Some(af),
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                filter_options: VideoFilterOptions {
                    tonemap_opencl: TonemapOpenclOptions {
                        tonemap: Some("hable".to_string()),
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
async fn deinterlace_anamorphic(
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
    #[values("aac", "ac3")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_vaapi_test_case(TestCase {
            fixture_name: "480p_h264_anamorphic.ts",
            params: TestOutputParams {
                audio_format: Some(af),
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                deinterlace: true,
                ..TestOutputParams::default()
            },
            expected_video_codec: vf.to_string(),
            expected_video_size: res,
            expected_audio_codec: af.to_string(),
        })
        .await;
    }
}

/// Tests pad with a 4:3 source -> 16:9 target, which forces pad_vaapi or pad_opencl.
#[rstest]
#[tokio::test]
#[ignore]
async fn pad(
    #[values("480p_h264.ts")] src: &'static str,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
    #[values("aac")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_vaapi_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                audio_format: Some(af),
                video_format: Some(vf),
                video_size: Some(FrameSize::from_str("1920x1080").unwrap()),
                bit_depth: Some(bpp),
                ..TestOutputParams::default()
            },
            expected_video_codec: vf.to_string(),
            expected_video_size: FrameSize::from_str("1920x1080").unwrap(),
            expected_audio_codec: af.to_string(),
        })
        .await;
    }
}

/// Tests pad_opencl by disabling pad_vaapi via ETV_TEST_DISABLED_FILTERS=pad_vaapi.
/// Run with: ETV_TEST_DISABLED_FILTERS=pad_vaapi cargo test --package ffpipeline --test vaapi pad_opencl -- --ignored
#[rstest]
#[tokio::test]
#[ignore]
async fn pad_opencl(
    #[values("480p_h264.ts")] src: &'static str,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
    #[values("aac")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_vaapi_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                audio_format: Some(af),
                video_format: Some(vf),
                video_size: Some(FrameSize::from_str("1920x1080").unwrap()),
                bit_depth: Some(bpp),
                ..TestOutputParams::default()
            },
            expected_video_codec: vf.to_string(),
            expected_video_size: FrameSize::from_str("1920x1080").unwrap(),
            expected_audio_codec: af.to_string(),
        })
        .await;
    }
}

/// The 1080p sources paired with a 1920x1080 output are the cases that matter: a
/// coded height of 1088 pads the decoder's surfaces, and an unscaled output means no
/// filter intervenes to launder the frame size. The rest are controls.
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
        run_vaapi_test_case(TestCase {
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

/// `Vaapi::best_overlay` upgrades a software overlay to `overlay_vaapi` whenever the
/// filter exists and the driver can blend BGRA, so without the env var below this is
/// just a duplicate of `watermark`. Users land on the software path on devices whose
/// VPP cannot blend BGRA.
///
/// What keeps the resulting hwdownload/overlay/hwupload chain safe is the explicit
/// `format=yuv420p` after hwdownload: it forces a real conversion, which reallocates
/// the frame at the link size instead of the decoder's padded surface height. Drop
/// that filter as redundant and 1080p h264 fails with "Failed to upload frame: -22".
///
/// Run with: ETV_TEST_DISABLED_FILTERS=overlay_vaapi cargo test --package ffpipeline --test vaapi watermark_software_overlay -- --ignored
#[rstest]
#[tokio::test]
#[ignore]
async fn watermark_software_overlay(
    #[values("1080p_h264.ts", "1080p_hevc_10.ts", "720p_h264.ts")] src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_vaapi_test_case(TestCase {
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

/// Exercises the `-ignore_loop 0` input branch instead of the still-image `-loop 1`.
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
        run_vaapi_test_case(TestCase {
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

/// 1440 and 854 are not 64-aligned, so the encoder has to signal the difference from
/// the coded size with a conformance window. Every other output size in the suite is
/// 64-aligned in width and only exercises the height half of that. Inert except on
/// drivers reporting VASurfaceAttribAlignmentSize, so this needs an AMD runner.
#[rstest]
#[tokio::test]
#[ignore]
async fn encode_alignment(
    #[values("1080p_h264.ts", "480p_h264.ts")] src: &'static str,
    #[values("1440x1080", "854x480", "1920x1080")] res: FrameSize,
    #[values(("hevc", 8), ("hevc", 10), ("h264", 8))] vf: (&'static str, u8),
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_vaapi_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                ..TestOutputParams::default()
            },
            expected_video_codec: vf.to_string(),
            expected_video_size: res,
            expected_audio_codec: AudioFormat::Aac.to_string(),
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
