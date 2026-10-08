use std::str::FromStr;

use ffpipeline::copy_decision::{CopyBlocker, CopyDecision, CopyDecisions};
use ffpipeline::frame_rate::FrameRate;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::input::{PeriodicClock, PeriodicTiming, WatermarkTiming};
use ffpipeline::output_settings::{
    AudioLoudnessSettings, CopyPolicy, LibplaceboOptions, TonemapOpenclOptions, TonemapOptions,
    VideoFilterOptions,
};
use ffpipeline::pipeline::{AudioFormat, EncodeFormat, VideoFormat};

use super::*;

/// Every suite, software included, runs these tests. Accels that can't do something
/// natively must still produce correct output through their fallback, and
/// `assert_accel_usage` checks that the fallback agrees with the capability probe.
///
/// `$accel` is an async fn returning the `Option<HardwareAccel>` under test.
#[macro_export]
macro_rules! shared_tests {
    ($accel:ident) => {
        #[::rstest::rstest]
        #[::tokio::test]
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
            #[values("1920x1080", "1280x720")] res: ::ffpipeline::frame_size::FrameSize,
            #[values(("mpeg2video", 8), ("h264", 8), ("hevc", 8), ("hevc", 10))] vf: (
                &'static str,
                u8,
            ),
            #[values("aac", "ac3")] af: ::ffpipeline::pipeline::AudioFormat,
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::transcode(src, res, vf, af),
            )
            .await;
        }

        /// 1440 and 854 are not 64-aligned, so an encoder that pads the coded size has to
        /// signal the difference with a conformance window. The other output sizes in the
        /// suite are 64-aligned in width and only exercise the height half of that. On
        /// VAAPI this is inert except on drivers reporting VASurfaceAttribAlignmentSize.
        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn encode_alignment(
            #[values("1080p_h264.ts", "480p_h264.ts")] src: &'static str,
            #[values("1440x1080", "854x480", "1920x1080")] res: ::ffpipeline::frame_size::FrameSize,
            #[values(("hevc", 8), ("hevc", 10), ("h264", 8))] vf: (&'static str, u8),
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::transcode(
                    src,
                    res,
                    vf,
                    ::ffpipeline::pipeline::AudioFormat::Aac,
                ),
            )
            .await;
        }

        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn rotated(
            #[values("1920x1080", "1280x720")] res: ::ffpipeline::frame_size::FrameSize,
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::transcode(
                    "720p_h264_rotated.mp4",
                    res,
                    ("h264", 8),
                    ::ffpipeline::pipeline::AudioFormat::Aac,
                ),
            )
            .await;
        }

        /// Target size differs from the source; copy must ignore it.
        #[::tokio::test]
        #[ignore]
        async fn codec_copy() {
            $crate::common::shared::run($accel().await, $crate::common::shared::codec_copy()).await;
        }

        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn copy_graphics(
            #[values(None, Some(::ffpipeline::frame_size::FrameSize { width: 1280, height: 720 }))]
            res: Option<::ffpipeline::frame_size::FrameSize>,
        ) {
            $crate::common::shared::run($accel().await, $crate::common::shared::copy_graphics(res))
                .await;
        }

        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn copy_image_subtitle(
            #[values(None, Some(::ffpipeline::frame_size::FrameSize { width: 1280, height: 720 }))]
            res: Option<::ffpipeline::frame_size::FrameSize>,
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::copy_image_subtitle(res),
            )
            .await;
        }

        #[::tokio::test]
        #[ignore]
        async fn copy_text_subtitle() {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::copy_text_subtitle(),
            )
            .await;
        }

        /// formats `PixelFormat` does not model
        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn copy_still_image(
            #[values("background.png", "background_rgb24.png")] src: &'static str,
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::copy_still_image(src),
            )
            .await;
        }

        #[::tokio::test]
        #[ignore]
        async fn copy_still_image_alpha() {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::copy_still_image_alpha(),
            )
            .await;
        }

        #[::tokio::test]
        #[ignore]
        async fn copy_generated_audio() {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::copy_generated_audio(),
            )
            .await;
        }

        #[::tokio::test]
        #[ignore]
        async fn copy_codec_not_allowed() {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::copy_codec_not_allowed(),
            )
            .await;
        }

        #[::tokio::test]
        #[ignore]
        async fn copy_codec_allowed() {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::copy_codec_allowed(),
            )
            .await;
        }

        #[::tokio::test]
        #[ignore]
        async fn copy_dv5() {
            $crate::common::shared::run($accel().await, $crate::common::shared::copy_dv5()).await;
        }

        #[::tokio::test]
        #[ignore]
        async fn loudness_normalization() {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::loudness_normalization(),
            )
            .await;
        }

        #[::rstest::rstest]
        #[case::down("1080p_h264.ts", "24", false)]
        #[case::up_ntsc("720p_h264.ts", "60000/1001", false)]
        #[case::deinterlaced("1080i_h264.ts", "25", true)]
        #[case::hdr("1080p_hevc_10_hdr.ts", "30", false)]
        #[::tokio::test]
        #[ignore]
        async fn custom_frame_rate(
            #[case] src: &'static str,
            #[case] rate: &'static str,
            #[case] deinterlace: bool,
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::custom_frame_rate(src, rate, deinterlace),
            )
            .await;
        }

        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn copy_frame_rate(#[values("30", "30000/1001")] rate: &'static str) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::copy_frame_rate(rate),
            )
            .await;
        }

        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn tonemap_hdr(
            #[values("1080p_hevc_10_hdr.ts", "1080p_hevc_10_hdr_4x3.ts")] src: &'static str,
            #[values("2560x1440", "1920x1080", "1280x720")]
            res: ::ffpipeline::frame_size::FrameSize,
            #[values(("h264", 8), ("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
            #[values("aac", "ac3")] af: ::ffpipeline::pipeline::AudioFormat,
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::tonemap(src, res, vf, af, false),
            )
            .await;
        }

        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn tonemap_hdr_watermark(
            #[values("1080p_hevc_10_hdr.ts", "1080p_hevc_10_hdr_4x3.ts")] src: &'static str,
            #[values("2560x1440", "1920x1080", "1280x720")]
            res: ::ffpipeline::frame_size::FrameSize,
            #[values(("h264", 8), ("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
            #[values("aac", "ac3")] af: ::ffpipeline::pipeline::AudioFormat,
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::tonemap(src, res, vf, af, true),
            )
            .await;
        }

        #[::rstest::rstest]
        #[::tokio::test]
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
            #[values("1920x1080", "1280x720")] res: ::ffpipeline::frame_size::FrameSize,
            #[values(("h264", 8), ("hevc", 8), ("hevc", 10))] vf: (&'static str, u8),
            #[values("aac", "ac3")] af: ::ffpipeline::pipeline::AudioFormat,
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::tonemap(src, res, vf, af, false),
            )
            .await;
        }

        /// `run_test_case` checks the bars. Upscale tonemaps before pad; downscale pads first.
        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn pad_color(
            #[values(
                "480p_h264.ts",
                "480p_hevc_10.ts",
                "1080p_hevc_10_bt2020_4x3.ts",
                "1080p_hevc_10_hdr_4x3.ts",
                "1080p_hevc_10_hdr_scope.ts"
            )]
            src: &'static str,
            #[values("2560x1440", "1920x1080", "1280x720")]
            res: ::ffpipeline::frame_size::FrameSize,
            #[values(("h264", 8), ("hevc", 10))] vf: (&'static str, u8),
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::tonemap(
                    src,
                    res,
                    vf,
                    ::ffpipeline::pipeline::AudioFormat::Aac,
                    false,
                ),
            )
            .await;
        }

        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn deinterlace(
            #[values("480i_h264.ts", "480i_h264_anamorphic.ts", "480i_mpeg2.ts")] src: &'static str,
            #[values("1920x1080", "1280x720")] res: ::ffpipeline::frame_size::FrameSize,
            #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
            #[values("aac", "ac3")] af: ::ffpipeline::pipeline::AudioFormat,
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::deinterlace(src, res, vf, af, true),
            )
            .await;
        }

        /// Checks the output pixels for combing, since encoder field tags can't prove
        /// that deinterlacing actually happened.
        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn deinterlace_motion(
            #[values("640x480", "854x480", "1920x1080")] res: ::ffpipeline::frame_size::FrameSize,
            #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::deinterlace(
                    "480i_h264_motion.ts",
                    res,
                    vf,
                    ::ffpipeline::pipeline::AudioFormat::Aac,
                    true,
                ),
            )
            .await;
        }

        /// 16:9 interlaced source transcoded with deinterlacing off: no pad is needed, so
        /// hardware pipelines keep decoded frames on the device all the way to the encoder.
        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn interlaced_no_deinterlace(
            #[values("1080i_h264.ts")] src: &'static str,
            #[values("1920x1080", "1280x720")] res: ::ffpipeline::frame_size::FrameSize,
            #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::deinterlace(
                    src,
                    res,
                    vf,
                    ::ffpipeline::pipeline::AudioFormat::Aac,
                    false,
                ),
            )
            .await;
        }

        /// A 1080p source has a coded height of 1088, so 1920x1080 output catches overlays
        /// that blend into the padded surface and leak it to the encoder, e.g. overlay_cuda
        /// (trac #11674), via the height assertion in `assert_video`.
        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn watermark(
            #[values(
                "1080p_h264.ts",
                "1080p_hevc_10.ts",
                "720p_h264.ts",
                "480p_h264.ts",
                "480p_h264_anamorphic.ts",
                "480p_h264_sps_change.ts"
            )]
            src: &'static str,
            #[values("1920x1080", "1280x720")] res: ::ffpipeline::frame_size::FrameSize,
            #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::watermark(
                    src,
                    res,
                    vf,
                    $crate::common::TestWatermark::default(),
                ),
            )
            .await;
        }

        /// Exercises the `-ignore_loop 0` input branch instead of the single-frame still-image branch.
        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn watermark_animated(
            #[values("1080p_h264.ts", "480p_h264_anamorphic.ts")] src: &'static str,
            #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::watermark(
                    src,
                    "1920x1080".parse().unwrap(),
                    vf,
                    $crate::common::shared::animated_watermark(),
                ),
            )
            .await;
        }

        /// Exercises a watermark without alpha, which needs an alpha channel before opacity is applied.
        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn watermark_no_alpha(
            #[values("1080p_h264.ts", "480p_h264_anamorphic.ts")] src: &'static str,
            #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::watermark(
                    src,
                    "1920x1080".parse().unwrap(),
                    vf,
                    $crate::common::shared::no_alpha_watermark(),
                ),
            )
            .await;
        }

        /// Exercises fades over a still image, which need the looped (repeated) frames to carry timestamps.
        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn watermark_periodic(
            #[values("1080p_h264.ts", "720p_h264.ts", "480p_h264_anamorphic.ts")] src: &'static str,
            #[values(("h264", 8), ("hevc", 8))] vf: (&'static str, u8),
        ) {
            $crate::common::shared::run(
                $accel().await,
                $crate::common::shared::watermark(
                    src,
                    "1920x1080".parse().unwrap(),
                    vf,
                    $crate::common::shared::periodic_watermark(),
                ),
            )
            .await;
        }

        /// The item ends with the audio, 101 ms after the video and off a frame boundary. With an
        /// endless fade loop, overlay_vaapi kept emitting frames at the last main pts after main
        /// EOF; the encoder dropped them all and ffmpeg never reached -t.
        #[::tokio::test]
        #[ignore]
        async fn watermark_periodic_audio_longer() {
            let mut test_case = $crate::common::shared::watermark(
                "720p_h264_audio_longer.mkv",
                "1920x1080".parse().unwrap(),
                ("h264", 8),
                $crate::common::shared::periodic_watermark(),
            );
            test_case.params.in_point = ::std::time::Duration::from_millis(2017);
            test_case.params.duration = ::std::time::Duration::from_millis(2084);
            $crate::common::shared::run($accel().await, test_case).await;
        }

        #[::rstest::rstest]
        #[::tokio::test]
        #[ignore]
        async fn canvas(
            #[values(("ffv1", "bgra"), ("ffv1", "yuva420p"), ("ffv1", "yuva444p"), ("png", "rgba"))]
            source: (&'static str, &'static str),
        ) {
            let accel = $accel().await;
            if let Some(env) = $crate::common::test_env().await {
                $crate::common::run_canvas_test(env, accel, source.0, source.1).await;
            }
        }

        #[::tokio::test]
        #[ignore]
        async fn loudness_normalization_with_realtime_canvas() {
            let accel = $accel().await;
            if let Some(env) = $crate::common::test_env().await {
                $crate::common::run_loudnorm_canvas_test(env, accel).await;
            }
        }
    };
}

pub async fn run(accel: Option<HardwareAccel>, mut test_case: TestCase) {
    if let Some(env) = test_env().await {
        test_case.params.accel = accel;
        run_test_case(env, test_case).await;
    }
}

pub fn transcode(src: &'static str, res: FrameSize, vf: (&str, u8), af: AudioFormat) -> TestCase {
    let (video_format, bit_depth) = vf;
    let video_format = EncodeFormat::from_str(video_format).unwrap();
    TestCase {
        fixture_name: src,
        audio_source: None,
        subtitle_fixture: None,
        params: TestOutputParams {
            audio_format: af,
            video_format,
            video_size: Some(res),
            bit_depth,
            ..TestOutputParams::default()
        },
        expected_video_codec: video_format.to_string(),
        expected_video_size: res,
        expected_audio_codec: af.to_string(),
        expected_copy: CopyDecisions::default(),
        burned_point: None,
    }
}

fn source_sized(src: &'static str, size: FrameSize) -> TestCase {
    TestCase {
        fixture_name: src,
        audio_source: None,
        subtitle_fixture: None,
        params: TestOutputParams::default(),
        expected_video_codec: String::from("h264"),
        expected_video_size: size,
        expected_audio_codec: String::from("aac"),
        expected_copy: CopyDecisions::default(),
        burned_point: None,
    }
}

const SIZE_1080P: FrameSize = FrameSize {
    width: 1920,
    height: 1080,
};

const SIZE_720P: FrameSize = FrameSize {
    width: 1280,
    height: 720,
};

fn copy(src: &'static str, size: FrameSize) -> TestCase {
    let mut test_case = source_sized(src, size);
    test_case.params.video_copy = Some(CopyPolicy::default());
    test_case.params.audio_copy = Some(CopyPolicy::default());
    test_case.expected_copy = CopyDecisions {
        video: Some(CopyDecision::Copy),
        audio: Some(CopyDecision::Copy),
    };
    test_case
}

fn blocked(blockers: Vec<CopyBlocker>) -> Option<CopyDecision> {
    Some(CopyDecision::Transcode(blockers))
}

pub fn codec_copy() -> TestCase {
    let mut test_case = copy("720p_h264.ts", SIZE_720P);
    test_case.params.video_size = Some(SIZE_1080P);
    test_case
}

/// No target size: the transcode keeps the source size.
pub fn copy_graphics(res: Option<FrameSize>) -> TestCase {
    let mut test_case = copy("1080p_h264.ts", res.unwrap_or(SIZE_1080P));
    test_case.params.video_size = res;
    test_case.params.watermark = Some(TestWatermark::default());
    test_case.expected_copy.video = blocked(vec![CopyBlocker::GraphicsLayers]);
    test_case
}

/// No target size: the subtitle burns in at the source size.
pub fn copy_image_subtitle(res: Option<FrameSize>) -> TestCase {
    let size = res.unwrap_or(SIZE_1080P);
    let mut test_case = copy("1080p_h264.ts", size);
    test_case.params.video_size = res;
    test_case.subtitle_fixture = Some("subtitle_pgs.sup");
    // inside the fixture box (760..1160 x 940..1020 at 1080p), over the dark blue bar
    test_case.burned_point = Some((1060 * size.width / 1920, 980 * size.height / 1080));
    test_case.expected_copy.video = blocked(vec![CopyBlocker::ImageSubtitle]);
    test_case
}

/// Copy drops the subtitles filter silently; a size change makes that visible.
pub fn copy_text_subtitle() -> TestCase {
    let mut test_case = copy("1080p_h264.ts", SIZE_720P);
    test_case.params.video_size = Some(SIZE_720P);
    test_case.subtitle_fixture = Some("subtitle.srt");
    test_case.expected_copy.video = blocked(vec![CopyBlocker::BurnedSubtitle]);
    test_case
}

pub fn copy_still_image(src: &'static str) -> TestCase {
    let mut test_case = copy(src, SIZE_1080P);
    test_case.audio_source = Some(TestAudioSource::Fixture("720p_h264.ts"));
    test_case.expected_copy.video = blocked(vec![
        CopyBlocker::CodecNotAllowed(String::from("png")),
        CopyBlocker::StillImage,
    ]);
    test_case
}

pub fn copy_still_image_alpha() -> TestCase {
    let mut test_case = copy("background_alpha.png", SIZE_720P);
    test_case.audio_source = Some(TestAudioSource::Fixture("720p_h264.ts"));
    test_case.expected_copy.video = blocked(vec![
        CopyBlocker::CodecNotAllowed(String::from("png")),
        CopyBlocker::StillImage,
    ]);
    test_case
}

pub fn copy_generated_audio() -> TestCase {
    let mut test_case = copy("720p_h264.ts", SIZE_720P);
    test_case.audio_source = Some(TestAudioSource::Lavfi(
        "anullsrc=channel_layout=stereo:sample_rate=48000",
    ));
    test_case.expected_copy.audio = blocked(vec![
        CopyBlocker::GeneratedSource,
        CopyBlocker::CodecNotAllowed(String::from("pcm_s16le")),
    ]);
    test_case
}

/// mpeg2video is not in the default copy list.
pub fn copy_codec_not_allowed() -> TestCase {
    let mut test_case = copy(
        "480i_mpeg2.ts",
        FrameSize {
            width: 640,
            height: 480,
        },
    );
    test_case.params.audio_copy = Some(CopyPolicy {
        formats: vec![String::from("ac3")],
    });
    test_case.params.audio_format = AudioFormat::Ac3;
    test_case.expected_audio_codec = String::from("ac3");
    test_case.expected_copy = CopyDecisions {
        video: blocked(vec![CopyBlocker::CodecNotAllowed(String::from(
            "mpeg2video",
        ))]),
        audio: blocked(vec![CopyBlocker::CodecNotAllowed(String::from("aac"))]),
    };
    test_case
}

pub fn copy_codec_allowed() -> TestCase {
    let mut test_case = copy(
        "480i_mpeg2.ts",
        FrameSize {
            width: 640,
            height: 480,
        },
    );
    test_case.params.video_copy = Some(CopyPolicy {
        formats: vec![VideoFormat::Mpeg2Video],
    });
    test_case.expected_video_codec = String::from("mpeg2video");
    test_case
}

/// `run_test_case` checks that the transcode tonemaps to SDR.
pub fn copy_dv5() -> TestCase {
    let mut test_case = copy("1080p_hevc_10_dv5.mp4", SIZE_1080P);
    test_case.expected_copy.video = blocked(vec![CopyBlocker::DolbyVision5]);
    test_case
}

pub fn loudness_normalization() -> TestCase {
    let mut test_case = source_sized("1080p_h264.ts", SIZE_1080P);
    test_case.params.loudness = Some(AudioLoudnessSettings::default());
    test_case
}

pub fn custom_frame_rate(src: &'static str, rate: &str, deinterlace: bool) -> TestCase {
    let mut test_case = transcode(src, SIZE_1080P, ("h264", 8), AudioFormat::Aac);
    test_case.params.frame_rate = FrameRate::parse_target(rate);
    test_case.params.deinterlace = deinterlace;
    test_case
}

pub fn copy_frame_rate(rate: &str) -> TestCase {
    let mut test_case = copy("720p_h264.ts", SIZE_720P);
    test_case.params.frame_rate = FrameRate::parse_target(rate);
    if rate != "30" {
        test_case.expected_copy.video = blocked(vec![CopyBlocker::FrameRateMismatch {
            source: String::from("30/1"),
            target: rate.to_owned(),
        }]);
    }
    test_case
}

/// Each accel reads a different one of these, so set them all to the same algorithm.
pub fn tonemap(
    src: &'static str,
    res: FrameSize,
    vf: (&str, u8),
    af: AudioFormat,
    watermark: bool,
) -> TestCase {
    let mut test_case = transcode(src, res, vf, af);
    test_case.params.filter_options = VideoFilterOptions {
        libplacebo: LibplaceboOptions {
            tonemapping: Some(String::from("hable")),
        },
        tonemap: TonemapOptions {
            tonemap: Some(String::from("hable")),
        },
        tonemap_opencl: TonemapOpenclOptions {
            tonemap: Some(String::from("hable")),
        },
        ..VideoFilterOptions::default()
    };
    if watermark {
        test_case.params.watermark = Some(TestWatermark::default());
    }
    test_case
}

pub fn deinterlace(
    src: &'static str,
    res: FrameSize,
    vf: (&str, u8),
    af: AudioFormat,
    deinterlace: bool,
) -> TestCase {
    let mut test_case = transcode(src, res, vf, af);
    test_case.params.deinterlace = deinterlace;
    test_case
}

pub fn watermark(
    src: &'static str,
    res: FrameSize,
    vf: (&str, u8),
    watermark: TestWatermark,
) -> TestCase {
    let mut test_case = transcode(src, res, vf, AudioFormat::Aac);
    test_case.params.watermark = Some(watermark);
    test_case
}

pub fn animated_watermark() -> TestWatermark {
    TestWatermark {
        fixture_name: "watermark.gif",
        ..TestWatermark::default()
    }
}

pub fn no_alpha_watermark() -> TestWatermark {
    TestWatermark {
        fixture_name: "watermark.jpg",
        opacity_percent: Some(50.0),
        ..TestWatermark::default()
    }
}

pub fn periodic_watermark() -> TestWatermark {
    TestWatermark {
        timing: Some(WatermarkTiming::Periodic(PeriodicTiming {
            clock: PeriodicClock::Content,
            frequency_ms: 2000,
            phase_offset_ms: None,
            disable_after_ms: None,
            fade_ms: Some(200),
            hold_ms: 400,
        })),
        ..TestWatermark::default()
    }
}
