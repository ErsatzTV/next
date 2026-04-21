use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ffpipeline::ffmpeg_info::FfmpegInfo;
use ffpipeline::frame_rate::FrameRate;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::input::{InputSettings, InputSource, LocalInputSource, ProbedInput};
use ffpipeline::output_format::OutputFormat;
use ffpipeline::output_settings::{AudioLoudnessSettings, AudioOutputSettings, OutputSettings};
use ffpipeline::pipeline::{AudioFormat, Hz, Kbps, Pipeline, VideoFormat, generate_pipeline};
use ffpipeline::probe::{ProbeDeps, ProbeResult, ProbeResultStream, Probeable};
use tokio::sync::OnceCell;

static TEST_ENV: OnceCell<Option<TestEnv>> = OnceCell::const_new();

pub struct TestEnv {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub ffmpeg_info: FfmpegInfo,
}

#[allow(dead_code)]
pub struct TestCase {
    pub fixture_name: &'static str,
    pub params: TestOutputParams,
    pub expected_video_codec: String,
    pub expected_video_size: FrameSize,
    pub expected_audio_codec: String,
}

pub async fn test_env() -> Option<&'static TestEnv> {
    TEST_ENV
        .get_or_init(|| async {
            let (ffmpeg, ffprobe) = find_binaries()?;
            let ffmpeg_info = load_ffmpeg_info(&ffmpeg).await;
            Some(TestEnv {
                ffmpeg,
                ffprobe,
                ffmpeg_info,
            })
        })
        .await
        .as_ref()
}

#[allow(dead_code)]
pub async fn run_test_case(test_env: &TestEnv, test_case: TestCase) {
    let dir = tempfile::tempdir().unwrap();
    let source = fixture_path(test_case.fixture_name);
    let probe = probe_file(&test_env.ffmpeg, &test_env.ffprobe, &source).await;

    let input = build_input(&source, probe, Duration::from_secs(1));
    let output = build_output(dir.path(), test_case.params);

    let mut pipeline = generate_pipeline(&test_env.ffmpeg_info, input, output).unwrap();
    pipeline.optimize();

    let (success, stderr) = run_ffmpeg_pipeline(&test_env.ffmpeg, &pipeline).await;
    assert!(success, "ffmpeg failed:\n{stderr}");

    let segment = find_first_segment(dir.path());
    let output_probe = probe_file(&test_env.ffmpeg, &test_env.ffprobe, &segment).await;
    assert_video(
        &output_probe,
        &test_case.expected_video_codec,
        test_case.expected_video_size.width,
        test_case.expected_video_size.height,
    );
    assert_audio(&output_probe, &test_case.expected_audio_codec);
}

pub fn find_ffmpeg() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("ETV_TEST_FFMPEG") {
        return Some(PathBuf::from(path));
    }

    which::which("ffmpeg").ok()
}

pub fn find_ffprobe() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("ETV_TEST_FFPROBE") {
        return Some(PathBuf::from(path));
    }
    which::which("ffprobe").ok()
}

pub fn find_binaries() -> Option<(PathBuf, PathBuf)> {
    Some((find_ffmpeg()?, find_ffprobe()?))
}

pub async fn load_ffmpeg_info(ffmpeg: &Path) -> FfmpegInfo {
    FfmpegInfo::load(ffmpeg, &[], &[])
        .await
        .expect("failed to load ffmpeg info")
}

pub fn fixture_path(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    assert!(path.exists(), "fixture not found: {}", path.display());
    path
}

pub async fn probe_file(ffmpeg: &Path, ffprobe: &Path, path: &Path) -> ProbeResult {
    let source = LocalInputSource {
        path: path.to_string_lossy().into_owned(),
    };
    let deps = ProbeDeps {
        ffmpeg_path: ffmpeg,
        ffprobe_path: ffprobe,
    };
    source.probe(&deps).await.expect("probe failed")
}

// --- Input/output builders ---

pub fn build_input(path: &Path, probe: ProbeResult, duration: Duration) -> InputSettings {
    let path_str = path.to_string_lossy().into_owned();
    InputSettings {
        audio_input: ProbedInput {
            input_source: InputSource::Local(LocalInputSource {
                path: path_str.clone(),
            }),
            probe_result: probe.clone(),
            in_point: Duration::ZERO,
            out_point: duration,
            audio_index: None,
            video_index: None,
        },
        video_input: ProbedInput {
            input_source: InputSource::Local(LocalInputSource { path: path_str }),
            probe_result: probe,
            in_point: Duration::ZERO,
            out_point: duration,
            audio_index: None,
            video_index: None,
        },
    }
}

#[allow(dead_code)]
pub struct TestOutputParams {
    pub video_format: Option<VideoFormat>,
    pub bit_depth: Option<u8>,
    pub video_bitrate: Option<Kbps>,
    pub video_buffer: Option<Kbps>,
    pub video_size: Option<FrameSize>,
    pub deinterlace: bool,
    pub audio_format: Option<AudioFormat>,
    pub audio_bitrate: Option<Kbps>,
    pub audio_channels: Option<u32>,
    pub loudness: Option<AudioLoudnessSettings>,
    pub accel: Option<HardwareAccel>,
    pub frame_rate: Option<FrameRate>,
}

impl Default for TestOutputParams {
    fn default() -> Self {
        Self {
            video_format: Some(VideoFormat::H264),
            bit_depth: Some(8),
            video_bitrate: Some(Kbps(5000)),
            video_buffer: Some(Kbps(10000)),
            video_size: None,
            deinterlace: false,
            audio_format: Some(AudioFormat::Aac),
            audio_bitrate: Some(Kbps(192)),
            audio_channels: Some(2),
            loudness: None,
            accel: None,
            frame_rate: None,
        }
    }
}

pub fn build_output(dir: &Path, params: TestOutputParams) -> OutputSettings {
    OutputSettings {
        audio: AudioOutputSettings {
            format: params.audio_format,
            bitrate: params.audio_bitrate,
            buffer: params.audio_bitrate.map(|b| Kbps(b.0 * 2)),
            channels: params.audio_channels,
            sample_rate: Some(Hz(48000)),
            loudness: params.loudness,
        },
        video_format: params.video_format,
        bit_depth: params.bit_depth,
        video_bitrate: params.video_bitrate,
        video_buffer: params.video_buffer,
        video_size: params.video_size,
        tonemap_algorithm: None,
        deinterlace: params.deinterlace,
        accel: params.accel,
        format: OutputFormat::Hls {
            playlist: dir.join("live.m3u8").to_string_lossy().into_owned(),
            segment_template: dir.join("segment_%03d.ts").to_string_lossy().into_owned(),
        },
        pts_offset: None,
        realtime: false,
        frame_rate: params.frame_rate,
    }
}

pub async fn run_ffmpeg_pipeline(ffmpeg: &Path, pipeline: &Pipeline) -> (bool, String) {
    let args = pipeline.args();
    let envs = pipeline.envs();

    let output = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(ffmpeg)
            .args(args.iter().map(Cow::as_ref))
            .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("ffmpeg timed out")
    .expect("failed to spawn ffmpeg");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.status.success(), stderr)
}

pub fn find_first_segment(dir: &Path) -> PathBuf {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("failed to read output dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().is_some_and(|ext| ext == "ts")
                && e.file_name().to_string_lossy().starts_with("segment_")
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    assert!(
        !entries.is_empty(),
        "no segment files found in {}",
        dir.display()
    );
    entries[0].path()
}

pub fn assert_video(probe: &ProbeResult, codec: &str, width: u32, height: u32) {
    let video = probe
        .streams
        .iter()
        .find_map(|s| match s {
            ProbeResultStream::Video(v) => Some(v),
            _ => None,
        })
        .expect("no video stream found in output");
    assert_eq!(video.codec, codec, "unexpected video codec");
    assert_eq!(video.width, width, "unexpected video width");
    assert_eq!(video.height, height, "unexpected video height");
}

pub fn assert_audio(probe: &ProbeResult, codec: &str) {
    let audio = probe
        .streams
        .iter()
        .find_map(|s| match s {
            ProbeResultStream::Audio(a) => Some(a),
            _ => None,
        })
        .expect("no audio stream found in output");
    assert_eq!(audio.codec, codec, "unexpected audio codec");
}
