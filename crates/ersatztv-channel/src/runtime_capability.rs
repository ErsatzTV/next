use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use ffpipeline::capabilities;
use ffpipeline::ffmpeg_info::FfmpegInfo;
use ffpipeline::hw_accel::{HardwareAccel as PipelineAccel, HwAccel};
use ffpipeline::pipeline::{EncodeFormat, PixelFormat};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{ChannelConfig, DEFAULT_VAAPI_DEVICE, HardwareAccel, VaapiDriver, VideoFormat};
use crate::error::ChannelError;

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const FFMPEG_TIMEOUT: Duration = Duration::from_secs(30);

const DECODE_CODECS: [&str; 7] = ["av1", "h264", "hevc", "mpeg2video", "vc1", "vp8", "vp9"];
const DECODE_PIXEL_FORMATS: [&str; 9] = [
    "yuv420p",
    "yuv420p10le",
    "yuv420p12le",
    "yuv422p",
    "yuv422p10le",
    "yuv422p12le",
    "yuv444p",
    "yuv444p10le",
    "yuv444p12le",
];
const ENCODE_FORMATS: [VideoFormat; 3] = [
    VideoFormat::H264,
    VideoFormat::Hevc,
    VideoFormat::Mpeg2Video,
];
const ENCODE_BIT_DEPTHS: [u8; 2] = [8, 10];

/// What this machine and ffmpeg build can do with a channel config.
#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq)]
#[schemars(title = "RuntimeCapability")]
pub struct RuntimeCapability {
    /// The ffmpeg build the channel config selects.
    pub ffmpeg: FfmpegRuntime,
    /// One entry per probe of every accel this build can probe here, selected or not. VAAPI appears
    /// once per render node and driver; the others once. `configured` marks the config's entry.
    pub accels: Vec<AccelRuntime>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq)]
#[schemars(title = "FfmpegRuntime")]
pub struct FfmpegRuntime {
    /// Path from the channel config, or `ffmpeg` (found on `PATH`) when it sets none.
    pub path: String,
    /// Version from `ffmpeg -version`, e.g. `n8.1.2-etv.4`, or `null` when it couldn't be read.
    pub version: Option<String>,
    /// Hardware accels the ffmpeg build lists (`ffmpeg -hwaccels`).
    pub hwaccels: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq)]
#[schemars(title = "AccelRuntime")]
pub struct AccelRuntime {
    /// The `normalization.video.accel` value.
    pub accel: String,
    /// Whether this is the accel, device and driver the channel config selects.
    pub configured: bool,
    /// Whether the channel can use this accel. `decode` and `encode` are empty unless it is
    /// `available`.
    pub status: AccelStatus,
    /// Why the accel isn't available, or `null`.
    pub error: Option<String>,
    /// The device probed: the VAAPI render node, or the AMF adapter (`<index>: <description>`, or
    /// the configured index when the probe failed). `null` for other accels.
    pub device: Option<String>,
    /// The VAAPI driver probed, as a `normalization.video.vaapi_driver` value; `null` when libva
    /// selected it, and for other accels.
    pub driver: Option<String>,
    /// Source codecs the channel decodes in hardware. Each codec was checked with a typical profile
    /// for each pixel format; a stream's actual profile or interlacing can still make it decode in
    /// software. A codec or pixel format not listed decodes in software.
    pub decode: Vec<DecodeSupport>,
    /// `normalization.video.format` values the channel encodes in hardware, by
    /// `normalization.video.bit_depth`. Anything not listed encodes in software.
    pub encode: Vec<EncodeSupport>,
}

/// `available`: the channel uses this accel. `ffmpeg_missing`: the ffmpeg build lacks its hwaccel,
/// so the channel uses software. `probe_failed`: the device or driver couldn't be opened (see
/// `error`). `timeout`: the probe didn't finish. `skipped`: not probed, because an earlier probe of
/// the same accel didn't finish.
#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
#[schemars(title = "AccelStatus")]
pub enum AccelStatus {
    Available,
    FfmpegMissing,
    ProbeFailed,
    Timeout,
    Skipped,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq)]
#[schemars(title = "DecodeSupport")]
pub struct DecodeSupport {
    /// Codec name as ffprobe reports it (`codec_name`), e.g. `hevc`.
    pub format: String,
    /// Pixel formats as ffprobe reports them (`pix_fmt`), e.g. `yuv420p10le`.
    pub pixel_formats: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, PartialEq)]
#[schemars(title = "EncodeSupport")]
pub struct EncodeSupport {
    /// A `normalization.video.format` value.
    pub format: String,
    /// `normalization.video.bit_depth` values.
    pub bit_depths: Vec<u8>,
}

/// What a probe config lacks falls back to this: defaults throughout, no accel selected. The
/// probe never reads the playout folder or audio settings, so a partial config is enough.
pub fn base_config() -> Value {
    serde_json::json!({
        "playout": { "folder": std::env::temp_dir() },
        "ffmpeg": {},
        "normalization": { "audio": {}, "video": {} },
    })
}

pub async fn probe(config: &ChannelConfig) -> Result<RuntimeCapability, ChannelError> {
    let ffmpeg_path = config
        .ffmpeg
        .ffmpeg_path
        .as_deref()
        .unwrap_or(Path::new("ffmpeg"));
    let ffmpeg_info = tokio::time::timeout(
        FFMPEG_TIMEOUT,
        FfmpegInfo::load(
            ffmpeg_path,
            &config.ffmpeg.disabled_filters,
            &config.ffmpeg.preferred_filters,
        ),
    )
    .await
    .map_err(|_| {
        ChannelError::DescribeProbe(format!("{} did not answer", ffmpeg_path.display()))
    })??;
    let hwaccels = ffmpeg_info.hw_accels();

    let accels = platform_accels()
        .into_iter()
        .flat_map(|accel| match accel {
            HardwareAccel::Vaapi => probe_vaapi(config, &hwaccels),
            _ => vec![probe_accel(config, &accel, &hwaccels)],
        })
        .collect();

    Ok(RuntimeCapability {
        ffmpeg: FfmpegRuntime {
            path: ffmpeg_path.display().to_string(),
            version: ffmpeg_version(ffmpeg_path).await,
            hwaccels,
        },
        accels,
    })
}

fn platform_accels() -> Vec<HardwareAccel> {
    let all = [
        HardwareAccel::Amf,
        HardwareAccel::Cuda,
        HardwareAccel::Qsv,
        HardwareAccel::Rkmpp,
        HardwareAccel::Vaapi,
        HardwareAccel::VideoToolbox,
        HardwareAccel::Vulkan,
    ];
    all.into_iter()
        .filter(|accel| match accel {
            HardwareAccel::Amf => capabilities::amf::HAS_PROBE,
            HardwareAccel::Cuda => capabilities::nvidia::HAS_PROBE,
            HardwareAccel::Qsv => capabilities::qsv::HAS_PROBE,
            HardwareAccel::Rkmpp => capabilities::rkmpp::HAS_PROBE,
            HardwareAccel::Vaapi => capabilities::vaapi::HAS_PROBE,
            HardwareAccel::VideoToolbox => capabilities::videotoolbox::HAS_PROBE,
            HardwareAccel::Vulkan => capabilities::vulkan::HAS_PROBE,
        })
        .collect()
}

/// A wrong node or driver is the usual VAAPI problem, so try every combination.
fn probe_vaapi(config: &ChannelConfig, hwaccels: &[String]) -> Vec<AccelRuntime> {
    let video = &config.normalization.video;
    if !hwaccels.iter().any(|a| a == "vaapi") {
        return vec![probe_accel(config, &HardwareAccel::Vaapi, hwaccels)];
    }

    let configured_device = vaapi_device(config);
    let mut devices = render_nodes();
    if !devices.contains(&configured_device) {
        devices.push(configured_device.clone());
    }
    let drivers = [
        None,
        Some(VaapiDriver::Ihd),
        Some(VaapiDriver::I965),
        Some(VaapiDriver::RadeonSI),
    ];

    let mut entries = Vec::new();
    // libva reads the driver from a process-wide env var; a hung probe would race the next one
    let mut hung = false;
    for device in &devices {
        for driver in &drivers {
            let mut combo = config.clone();
            combo.normalization.video.vaapi_device = Some(device.clone());
            combo.normalization.video.vaapi_driver = driver.clone();
            let mut entry = if hung {
                AccelRuntime {
                    status: AccelStatus::Skipped,
                    error: Some(String::from("an earlier VAAPI probe did not finish")),
                    ..unprobed(&combo, &HardwareAccel::Vaapi)
                }
            } else {
                probe_accel(&combo, &HardwareAccel::Vaapi, hwaccels)
            };
            entry.configured = video.accel == Some(HardwareAccel::Vaapi)
                && *device == configured_device
                && *driver == video.vaapi_driver;
            hung |= entry.status == AccelStatus::Timeout;
            entries.push(entry);
        }
    }
    entries
}

fn render_nodes() -> Vec<PathBuf> {
    let mut nodes: Vec<PathBuf> = std::fs::read_dir("/dev/dri")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("renderD"))
        .map(|e| e.path())
        .collect();
    nodes.sort();
    nodes
}

fn vaapi_device(config: &ChannelConfig) -> PathBuf {
    config
        .normalization
        .video
        .vaapi_device
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_VAAPI_DEVICE))
}

fn unprobed(config: &ChannelConfig, accel: &HardwareAccel) -> AccelRuntime {
    let video = &config.normalization.video;
    AccelRuntime {
        accel: enum_name(accel),
        configured: video.accel.as_ref() == Some(accel),
        status: AccelStatus::Available,
        error: None,
        device: configured_device(config, accel),
        driver: match accel {
            HardwareAccel::Vaapi => video.vaapi_driver.as_ref().map(enum_name),
            _ => None,
        },
        decode: Vec::new(),
        encode: Vec::new(),
    }
}

fn probe_accel(config: &ChannelConfig, accel: &HardwareAccel, hwaccels: &[String]) -> AccelRuntime {
    let name = enum_name(accel);
    let mut runtime = unprobed(config, accel);

    if !hwaccels.contains(&name) {
        runtime.status = AccelStatus::FfmpegMissing;
        runtime.error = Some(format!("ffmpeg has no `{name}` hwaccel"));
        return runtime;
    }

    let thread_config = config.clone();
    let thread_accel = accel.clone();
    let probed = bounded(PROBE_TIMEOUT, move || {
        thread_accel
            .try_to_pipeline(&thread_config)
            .map(|a| support(&a))
    });

    match probed {
        Some(Ok((device, decode, encode))) => {
            runtime.device = device.or(runtime.device);
            runtime.decode = decode;
            runtime.encode = encode;
        }
        Some(Err(e)) => {
            runtime.status = AccelStatus::ProbeFailed;
            runtime.error = Some(e);
        }
        None => {
            runtime.status = AccelStatus::Timeout;
            runtime.error = Some(format!(
                "probe did not finish in {}s",
                PROBE_TIMEOUT.as_secs()
            ));
        }
    }
    runtime
}

/// Known before probing, so a failed probe still names its device.
fn configured_device(config: &ChannelConfig, accel: &HardwareAccel) -> Option<String> {
    let video = &config.normalization.video;
    match accel {
        HardwareAccel::Vaapi => Some(vaapi_device(config).display().to_string()),
        HardwareAccel::Amf => video.amf_device.map(|index| index.to_string()),
        _ => None,
    }
}

/// `None` on timeout; the thread keeps running.
// not spawn_blocking: runtime shutdown waits for blocking tasks, so a hung driver would hang describe
fn bounded<T: Send + 'static>(
    timeout: Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(timeout).ok()
}

fn support(accel: &PipelineAccel) -> (Option<String>, Vec<DecodeSupport>, Vec<EncodeSupport>) {
    let device = match accel {
        PipelineAccel::Vaapi(vaapi) => Some(vaapi.device.clone()),
        PipelineAccel::Amf(amf) => amf
            .capabilities
            .adapter()
            .map(|a| format!("{}: {}", a.index, a.description)),
        _ => None,
    };

    let decode = DECODE_CODECS
        .iter()
        .filter_map(|codec| {
            let pixel_formats: Vec<String> = DECODE_PIXEL_FORMATS
                .iter()
                .filter(|pix_fmt| {
                    let pixel_format = PixelFormat::parse(pix_fmt);
                    accel.can_decode(codec, typical_profile(codec, &pixel_format), &pixel_format)
                })
                .map(|pix_fmt| pix_fmt.to_string())
                .collect();
            (!pixel_formats.is_empty()).then(|| DecodeSupport {
                format: codec.to_string(),
                pixel_formats,
            })
        })
        .collect();

    let encode = ENCODE_FORMATS
        .iter()
        .filter_map(|format| {
            let bit_depths: Vec<u8> = ENCODE_BIT_DEPTHS
                .into_iter()
                .filter(|bit_depth| accel.can_encode(&EncodeFormat::from(*format), *bit_depth))
                .collect();
            (!bit_depths.is_empty()).then(|| EncodeSupport {
                format: enum_name(format),
                bit_depths,
            })
        })
        .collect();

    (device, decode, encode)
}

/// ffprobe profile names. Only VAAPI's decode check uses the profile.
fn typical_profile(codec: &str, pixel_format: &PixelFormat) -> &'static str {
    use ffpipeline::pipeline::Sampling;

    let bit_depth = pixel_format.bit_depth();
    match (codec, pixel_format.sampling(), bit_depth) {
        ("h264", Sampling::Yuv420, 8) => "high",
        ("h264", Sampling::Yuv420, _) => "high 10",
        ("h264", Sampling::Yuv422, _) => "high 4:2:2",
        ("h264", _, _) => "high 4:4:4 predictive",
        ("hevc", Sampling::Yuv420, 8) => "main",
        ("hevc", Sampling::Yuv420, 10) => "main 10",
        ("hevc", _, _) => "rext",
        ("mpeg2video", Sampling::Yuv420, _) => "main",
        ("mpeg2video", _, _) => "4:2:2",
        ("vc1", _, _) => "advanced",
        ("vp8", _, _) => "0",
        ("vp9", Sampling::Yuv420, 8) => "profile 0",
        ("vp9", Sampling::Yuv420, _) => "profile 2",
        ("vp9", _, 8) => "profile 1",
        ("vp9", _, _) => "profile 3",
        ("av1", Sampling::Yuv420, 8 | 10) => "main",
        ("av1", Sampling::Yuv444, 8 | 10) => "high",
        (_, _, _) => "professional",
    }
}

async fn ffmpeg_version(path: &Path) -> Option<String> {
    let output = tokio::time::timeout(
        FFMPEG_TIMEOUT,
        ersatztv_core::process::command(path)
            .arg("-version")
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    // "ffmpeg version <version> Copyright ..."
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()?
        .strip_prefix("ffmpeg version ")?
        .split_whitespace()
        .next()
        .map(String::from)
}

fn enum_name<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn bounded_returns_in_time() {
        assert_eq!(bounded(Duration::from_secs(5), || 7), Some(7));
    }

    #[test]
    fn bounded_gives_up_on_a_hung_call() {
        let started = Instant::now();
        let result = bounded(Duration::from_millis(100), || {
            std::thread::sleep(Duration::from_secs(60));
        });
        assert_eq!(result, None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
