#![cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod common;

use std::str::FromStr;

use common::*;
use ffpipeline::accel::cuda::Cuda;
use ffpipeline::capabilities::nvidia::NvidiaCapabilities;
use ffpipeline::capabilities::vulkan::VulkanCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::output_settings::{LibplaceboOptions, VideoFilterOptions};
use ffpipeline::pipeline::{AudioFormat, VideoFormat};
use rstest::rstest;
use tokio::sync::OnceCell;

static CUDA_ACCEL: OnceCell<Option<HardwareAccel>> = OnceCell::const_new();

async fn make_cuda_accel() -> Option<&'static HardwareAccel> {
    CUDA_ACCEL
        .get_or_init(|| async {
            let capabilities = NvidiaCapabilities::probe().ok()?;
            let vulkan = VulkanCapabilities::probe_for_nvidia(capabilities.device_uuid()).ok();
            Some(HardwareAccel::Cuda(Cuda::new(capabilities, vulkan)))
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
        "480p_h264_anamorphic.ts",
        "480p_h264_sps_change.ts"
    )]
    src: &'static str,
    #[values("1920x1080", "1280x720")] res: FrameSize,
    #[values(("h264", 8), ("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
    #[values("aac", "ac3")] af: AudioFormat,
) {
    let (vf_str, bpp) = vf;
    if let Ok(vf) = VideoFormat::from_str(vf_str) {
        run_cuda_test_case(TestCase {
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
        run_cuda_test_case(TestCase {
            fixture_name: "1080p_hevc_10_hdr.ts",
            params: TestOutputParams {
                audio_format: Some(af),
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                filter_options: VideoFilterOptions {
                    libplacebo: LibplaceboOptions {
                        tonemapping: Some("hable".to_string()),
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
        run_cuda_test_case(TestCase {
            fixture_name: src,
            params: TestOutputParams {
                audio_format: Some(af),
                video_format: Some(vf),
                video_size: Some(res),
                bit_depth: Some(bpp),
                filter_options: VideoFilterOptions {
                    libplacebo: LibplaceboOptions {
                        tonemapping: Some("hable".to_string()),
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

/// `overlay_cuda` blends in place, so it gets a writable buffer sized to the frames
/// context rather than the link -- 1920x1088 for a 1080p source, which then reaches
/// the encoder as a 1088-line output with a green bottom row (trac #11674). The
/// 1080p sources at 1920x1080 are the cases that catch it, via the height assertion
/// in `assert_video`. 10-bit sources fall back to the software overlay, since
/// `overlay_cuda` is 8-bit only.
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
        run_cuda_test_case(TestCase {
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
        run_cuda_test_case(TestCase {
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

async fn run_cuda_test_case(mut test_case: TestCase) {
    if let Some(env) = test_env().await {
        if !env.ffmpeg_info.has_hw_accel(&KnownHardwareAccel::Cuda) {
            panic!("cuda not available in ffmpeg");
        }

        let Some(accel) = make_cuda_accel().await else {
            panic!("no usable NVIDIA GPU found");
        };

        test_case.params.accel = Some(accel.clone());
        run_test_case(env, test_case).await;
    }
}
