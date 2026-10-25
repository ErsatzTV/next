#![cfg(target_os = "linux")]
mod common;

use std::path::PathBuf;

use common::shared::transcode;
use common::*;
use ffpipeline::accel::vaapi::{Vaapi, VaapiDriver};
use ffpipeline::capabilities::opencl::OpenCLCapabilities;
use ffpipeline::capabilities::vaapi::VaapiCapabilities;
use ffpipeline::capabilities::vulkan::VulkanCapabilities;
use ffpipeline::ffmpeg_info::{KnownHardwareAccel, KnownVideoFilter};
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::pipeline::AudioFormat;
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

fn probe_vaapi() -> Option<(
    String,
    Option<VaapiDriver>,
    VaapiCapabilities,
    OpenCLCapabilities,
)> {
    let device = find_vaapi_device()?;
    let device_str = device.to_str()?;
    let driver = find_vaapi_driver();

    let caps = VaapiCapabilities::probe(
        device_str,
        driver.as_ref().map(|d| d.to_string()).as_deref(),
    )
    .ok()
    .filter(|caps| caps.count() > 0)?;
    let opencl_caps = OpenCLCapabilities::probe().unwrap_or_default();
    Some((device_str.to_owned(), driver, caps, opencl_caps))
}

fn probe() -> Option<HardwareAccel> {
    let (device, driver, capabilities, opencl_capabilities) = probe_vaapi()?;
    let vulkan_capabilities = VulkanCapabilities::probe_for_vaapi(&device, &capabilities).ok();
    Some(HardwareAccel::Vaapi(Vaapi {
        device,
        driver,
        capabilities,
        opencl_capabilities,
        vulkan_capabilities,
        prefer_low_power: std::env::var("ETV_TEST_VAAPI_LOW_POWER").as_deref() != Ok("never"),
    }))
}

async fn accel() -> Option<HardwareAccel> {
    hardware_accel(KnownHardwareAccel::Vaapi, probe).await
}

shared_tests!(accel);

async fn run_vaapi_test_case(
    mut test_case: TestCase,
) -> Option<(&'static TestEnv, Vaapi, Vec<String>)> {
    let Some(HardwareAccel::Vaapi(vaapi)) = accel().await else {
        return None;
    };
    let env = test_env().await?;
    test_case.params.accel = Some(HardwareAccel::Vaapi(vaapi.clone()));
    let args = run_test_case(env, test_case).await;
    Some((env, vaapi, args))
}

/// 4:3 source to 16:9 output needs a pad; hiding pad_vaapi exercises the OpenCL
/// fallback, or the software pad on devices without OpenCL.
#[rstest]
#[tokio::test]
#[ignore]
async fn pad_opencl(#[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8)) {
    let mut test_case = transcode(
        "480p_h264.ts",
        "1920x1080".parse().unwrap(),
        vf,
        AudioFormat::Aac,
    );
    test_case.params.disabled_filters = vec!["pad_vaapi"];
    if let Some((env, vaapi, args)) = run_vaapi_test_case(test_case).await
        && vaapi.capabilities.can_map_to_opencl()
        && vaapi.opencl_capabilities.can_pad()
        && env
            .ffmpeg_info
            .has_video_filter(&KnownVideoFilter::PadOpencl)
    {
        assert!(
            args.join(" ").contains("pad_opencl"),
            "OpenCL can pad but the pipeline did not use pad_opencl"
        );
    }
}

/// `Vaapi::best_overlay` upgrades a software overlay to `overlay_vaapi` whenever the
/// filter exists and the driver can blend BGRA, so hiding the filter is the only way to
/// reach the software path users get on devices whose VPP cannot blend BGRA.
///
/// What keeps the resulting hwdownload/overlay/hwupload chain safe is the explicit
/// `format=yuv420p` after hwdownload: it forces a real conversion, which reallocates
/// the frame at the link size instead of the decoder's padded surface height. Drop
/// that filter as redundant and 1080p h264 fails with "Failed to upload frame: -22".
#[rstest]
#[tokio::test]
#[ignore]
async fn watermark_software_overlay(
    #[values("1080p_h264.ts", "1080p_hevc_10.ts", "720p_h264.ts")] src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
) {
    let mut test_case = common::shared::watermark(src, res, vf, TestWatermark::default());
    test_case.params.disabled_filters = vec!["overlay_vaapi"];
    run_vaapi_test_case(test_case).await;
}

/// Downscale pads before tonemap, so a software pad must upload to OpenCL.
#[rstest]
#[tokio::test]
#[ignore]
async fn pad_software_tonemap_opencl(
    #[values("1080p_hevc_10_hdr_scope.ts", "1080p_hevc_10_hdr_4x3.ts")] src: &'static str,
    #[values(("h264", 8), ("hevc", 10))] vf: (&'static str, u8),
) {
    let mut test_case = common::shared::tonemap(
        src,
        "1280x720".parse().unwrap(),
        vf,
        AudioFormat::Aac,
        false,
    );
    // libplacebo is preferred over OpenCL
    test_case.params.disabled_filters = vec!["pad_vaapi", "pad_opencl", "libplacebo"];
    if let Some((env, vaapi, args)) = run_vaapi_test_case(test_case).await
        && vaapi.capabilities.can_map_to_opencl()
        && vaapi.opencl_capabilities.can_tonemap()
        && env
            .ffmpeg_info
            .has_video_filter(&KnownVideoFilter::TonemapOpencl)
    {
        assert!(
            args.join(" ").contains("tonemap_opencl"),
            "OpenCL can tonemap but the pipeline did not use tonemap_opencl"
        );
    }
}

/// hides pad_vaapi, like Mesa
#[rstest]
#[tokio::test]
#[ignore]
async fn pad_tonemap_libplacebo(
    #[values("1080p_hevc_10_hdr_scope.ts", "1080p_hevc_10_hdr_4x3.ts")] src: &'static str,
) {
    let mut test_case = common::shared::tonemap(
        src,
        "1280x720".parse().unwrap(),
        ("h264", 8),
        AudioFormat::Aac,
        false,
    );
    test_case.params.disabled_filters = vec!["pad_vaapi", "pad_opencl"];
    if let Some((env, vaapi, args)) = run_vaapi_test_case(test_case).await
        && vaapi
            .vulkan_capabilities
            .as_ref()
            .is_some_and(|vk| vk.can_import_drm_prime())
        && env
            .ffmpeg_info
            .has_video_filter(&KnownVideoFilter::LibPlacebo)
        && env
            .ffmpeg_info
            .has_video_filter(&KnownVideoFilter::ScaleVulkan)
    {
        let args = args.join(" ");
        assert!(
            args.contains("libplacebo="),
            "Vulkan can import DRM_PRIME but the pipeline did not use libplacebo"
        );
        assert!(
            args.contains("fillcolor=black") && !args.contains("hwdownload"),
            "pad was not folded into libplacebo: {args}"
        );
    }
}
