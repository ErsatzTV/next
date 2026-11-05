use std::path::{Path, PathBuf};
use std::process::Command;

use ersatztv_channel::describe::Describe;
use ersatztv_channel::runtime_capability::{AccelStatus, RuntimeCapability};
use serde_json::json;

#[test]
fn prints_describe_document() {
    let output = Command::new(env!("CARGO_BIN_EXE_ersatztv-channel"))
        .arg("describe")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let printed: Describe = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(printed, Describe::new());
}

#[test]
fn committed_schema_is_current() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/describe.json");
    let committed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let generated = serde_json::to_value(schemars::schema_for!(Describe)).unwrap();
    assert_eq!(
        committed, generated,
        "regenerate: cargo run --bin gen-describe-schema > schema/describe.json"
    );
}

/// A partial config: the probe fills in everything it doesn't read.
fn probe(ffmpeg: &Path, accel: &str) -> RuntimeCapability {
    let dir = tempfile::tempdir().unwrap();
    let config = json!({
        "version": ersatztv_channel::config::SCHEMA.uri(),
        "ffmpeg": { "ffmpeg_path": ffmpeg },
        "normalization": { "video": { "accel": accel } },
    });
    let path = dir.path().join("channel.json");
    std::fs::write(&path, config.to_string()).unwrap();

    let runtime = run_probe(&[path.as_os_str()]);
    let configured = runtime.accels.iter().filter(|a| a.configured).count();
    let on_platform = runtime.accels.iter().any(|a| a.accel == accel);
    assert_eq!(configured, usize::from(on_platform), "{:?}", runtime.accels);
    runtime
}

fn run_probe(args: &[&std::ffi::OsStr]) -> RuntimeCapability {
    let output = Command::new(env!("CARGO_BIN_EXE_ersatztv-channel"))
        .args(["describe", "--probe"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let mut printed: Describe = serde_json::from_slice(&output.stdout).unwrap();
    let runtime = printed
        .runtime_capability
        .take()
        .expect("runtime_capability");
    assert_eq!(
        printed,
        Describe::new(),
        "--probe only adds runtime_capability"
    );
    runtime
}

/// an ffmpeg with no hwaccels, so no driver is probed
#[cfg(unix)]
fn fake_ffmpeg(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let ffmpeg = dir.join("ffmpeg");
    std::fs::write(
        &ffmpeg,
        "#!/bin/sh\n[ \"$1\" = -version ] && echo 'ffmpeg version n0.0-test Copyright'\nexit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(&ffmpeg, std::fs::Permissions::from_mode(0o755)).unwrap();
    ffmpeg
}

#[cfg(unix)]
#[test]
fn probe_without_ffmpeg_hwaccels() {
    let dir = tempfile::tempdir().unwrap();
    let ffmpeg = fake_ffmpeg(dir.path());

    let runtime = probe(&ffmpeg, "vaapi");
    assert_eq!(runtime.ffmpeg.path, ffmpeg.display().to_string());
    assert_eq!(runtime.ffmpeg.version.as_deref(), Some("n0.0-test"));
    assert!(runtime.ffmpeg.hwaccels.is_empty());
    assert!(!runtime.accels.is_empty());
    for accel in &runtime.accels {
        assert_eq!(accel.status, AccelStatus::FfmpegMissing, "{accel:?}");
        assert!(
            accel.decode.is_empty() && accel.encode.is_empty(),
            "{accel:?}"
        );
    }
    assert_eq!(
        runtime.accels.iter().any(|a| a.accel == "vaapi"),
        cfg!(target_os = "linux")
    );
}

#[test]
#[ignore = "needs ETV_TEST_FFMPEG (or ffmpeg on PATH); probes this machine's GPUs"]
fn probe_with_real_ffmpeg() {
    let ffmpeg =
        std::env::var_os("ETV_TEST_FFMPEG").map_or_else(|| PathBuf::from("ffmpeg"), PathBuf::from);
    for configured in ["vulkan", "vaapi"] {
        let runtime = probe(&ffmpeg, configured);
        assert!(runtime.ffmpeg.version.is_some());
        for accel in &runtime.accels {
            match accel.status {
                AccelStatus::Available => assert!(accel.error.is_none(), "{accel:?}"),
                _ => {
                    assert!(accel.error.is_some(), "{accel:?}");
                    assert!(
                        accel.decode.is_empty() && accel.encode.is_empty(),
                        "{accel:?}"
                    );
                }
            }
            if accel.status == AccelStatus::FfmpegMissing {
                assert!(!runtime.ffmpeg.hwaccels.contains(&accel.accel), "{accel:?}");
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn probe_without_config() {
    let dir = tempfile::tempdir().unwrap();
    let ffmpeg = fake_ffmpeg(dir.path());

    let runtime = run_probe(&["--ffmpeg".as_ref(), ffmpeg.as_os_str()]);
    assert_eq!(runtime.ffmpeg.path, ffmpeg.display().to_string());
    assert!(!runtime.accels.is_empty());
    assert!(
        runtime.accels.iter().all(|a| !a.configured),
        "{:?}",
        runtime.accels
    );
}
