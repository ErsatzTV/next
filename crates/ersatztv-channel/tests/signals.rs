#![cfg(target_os = "linux")]

use std::ffi::OsString;
use std::fs::File;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const CHANNEL: &str = env!("CARGO_BIN_EXE_ersatztv-channel");

#[test]
#[ignore = "requires ffmpeg"]
fn sigterm_stops_ffmpeg() {
    let fixture = Fixture::new();
    let mut channel = fixture.channel(&[]).spawn().unwrap();
    let ffmpeg = wait_for_transcode(channel.id());

    signal(channel.id(), "TERM");

    assert!(wait_exit(&mut channel, Duration::from_secs(5)).success());
    assert!(
        wait_gone(ffmpeg, Duration::from_secs(2)),
        "ffmpeg outlived the channel"
    );
}

#[test]
#[ignore = "requires ffmpeg"]
fn sigkill_stops_ffmpeg() {
    let fixture = Fixture::new();
    let mut channel = fixture.channel(&[]).spawn().unwrap();
    let ffmpeg = wait_for_transcode(channel.id());

    channel.kill().unwrap();
    channel.wait().unwrap();

    assert!(
        wait_gone(ffmpeg, Duration::from_secs(2)),
        "ffmpeg outlived the channel"
    );
}

#[test]
#[ignore = "requires ffmpeg"]
fn killed_parent_stops_channel() {
    let fixture = Fixture::new();
    let (mut parent, channel) = fixture.spawn_under_sh(&[]);
    let ffmpeg = wait_for_transcode(channel);

    parent.kill().unwrap();
    parent.wait().unwrap();

    assert!(
        wait_gone(channel, Duration::from_secs(3)),
        "channel outlived its parent"
    );
    assert!(
        wait_gone(ffmpeg, Duration::from_secs(2)),
        "ffmpeg outlived the channel"
    );
}

#[test]
#[ignore = "requires ffmpeg"]
fn detached_survives_killed_parent() {
    let fixture = Fixture::new();
    let (mut parent, channel) = fixture.spawn_under_sh(&["--detached"]);
    wait_for_transcode(channel);

    parent.kill().unwrap();
    parent.wait().unwrap();

    // over two poll intervals
    std::thread::sleep(Duration::from_millis(2500));
    assert!(
        is_running(channel),
        "detached channel stopped with its parent"
    );
}

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let content = root.join("test.mp4");
        let status = Command::new(test_binary("ETV_TEST_FFMPEG", "ffmpeg"))
            .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi"])
            .args([
                "-i",
                "testsrc2=s=1280x720:r=30",
                "-f",
                "lavfi",
                "-i",
                "sine",
            ])
            .args(["-t", "120", "-c:v", "libx264", "-preset", "ultrafast"])
            .args(["-c:a", "aac", "-shortest"])
            .arg(&content)
            .status()
            .unwrap();
        assert!(status.success(), "ffmpeg failed to create test content");

        let playout = root.join("playout");
        std::fs::create_dir(&playout).unwrap();
        let start = OffsetDateTime::now_utc() - Duration::from_secs(1);
        let finish = start + Duration::from_secs(120);
        let items = json!({
            "version": ersatztv_playout::playout::SCHEMA.uri(),
            "items": [{
                "id": "1",
                "start": start.format(&Rfc3339).unwrap(),
                "finish": finish.format(&Rfc3339).unwrap(),
                "source": { "source_type": "local", "path": content },
            }],
        });
        let name = format!(
            "{}_{}.json",
            start.unix_timestamp(),
            finish.unix_timestamp()
        );
        std::fs::write(playout.join(name), items.to_string()).unwrap();

        // a bare name would resolve relative to the config; unset means PATH
        let mut ffmpeg = json!({ "disabled_filters": [] });
        if let Some(path) = std::env::var_os("ETV_TEST_FFMPEG") {
            ffmpeg["ffmpeg_path"] = json!(PathBuf::from(path));
        }
        if let Some(path) = std::env::var_os("ETV_TEST_FFPROBE") {
            ffmpeg["ffprobe_path"] = json!(PathBuf::from(path));
        }
        let config = json!({
            "version": ersatztv_channel::config::SCHEMA.uri(),
            "playout": { "folder": playout },
            "ffmpeg": ffmpeg,
            "normalization": {
                "audio": {
                    "mode": "transcode", "format": "aac", "bitrate_kbps": 192, "buffer_kbps": 384,
                    "channels": 2, "sample_rate_hz": 48000,
                },
                "video": {
                    "mode": "transcode", "format": "h264", "bit_depth": 8, "width": 1280,
                    "height": 720, "bitrate_kbps": 2000, "buffer_kbps": 4000, "accel": null,
                },
            },
        });
        std::fs::write(root.join("channel.json"), config.to_string()).unwrap();

        let output = root.join("out");
        std::fs::create_dir(&output).unwrap();
        File::create(output.join(ersatztv_core::HEARTBEAT_FILE_NAME)).unwrap();

        Self { dir }
    }

    fn run_args(&self, extra: &[&str]) -> Vec<OsString> {
        let root = self.dir.path();
        let mut args: Vec<OsString> = vec!["run".into()];
        args.extend(extra.iter().map(OsString::from));
        args.extend([
            "--output-folder".into(),
            root.join("out").into(),
            "--number".into(),
            "1".into(),
            root.join("channel.json").into(),
        ]);
        args
    }

    fn channel(&self, extra: &[&str]) -> Command {
        let mut command = Command::new(CHANNEL);
        command.args(self.run_args(extra));
        self.log_to(&mut command);
        command
    }

    /// sh stands in for a spawner that dies without stopping the channel
    fn spawn_under_sh(&self, extra: &[&str]) -> (Child, u32) {
        let mut command = Command::new("sh");
        command
            .args(["-c", "\"$0\" \"$@\" & wait", CHANNEL])
            .args(self.run_args(extra));
        self.log_to(&mut command);
        let parent = command.spawn().unwrap();
        let channel = wait_for_child(parent.id(), |cmdline| cmdline.starts_with(CHANNEL));
        (parent, channel)
    }

    fn log_to(&self, command: &mut Command) {
        let log = File::create(self.dir.path().join("channel.log")).unwrap();
        command.stdout(Stdio::null()).stderr(log);
    }
}

impl Drop for Fixture {
    // a failed assertion would otherwise leave the channel and ffmpeg running
    fn drop(&mut self) {
        let root = self.dir.path().to_string_lossy().into_owned();
        for pid in pids() {
            if cmdline(pid).is_some_and(|cmdline| cmdline.contains(&root)) {
                signal(pid, "KILL");
            }
        }
    }
}

fn test_binary(env: &str, default: &str) -> PathBuf {
    std::env::var_os(env).map_or_else(|| PathBuf::from(default), PathBuf::from)
}

// the channel runs short ffmpeg probes before the transcode
fn wait_for_transcode(channel: u32) -> u32 {
    wait_for_child(channel, |cmdline| cmdline.contains("hls_time"))
}

fn wait_for_child(parent: u32, matches: impl Fn(&str) -> bool) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let child = pids().find(|&pid| {
            parent_pid(pid) == Some(parent) && cmdline(pid).is_some_and(|cmdline| matches(&cmdline))
        });
        if let Some(child) = child {
            return child;
        }
        assert!(Instant::now() < deadline, "no matching child of {parent}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn wait_exit(child: &mut Child, timeout: Duration) -> ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "{} didn't exit", child.id());
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_gone(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while is_running(pid) {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

fn signal(pid: u32, name: &str) {
    let _ = Command::new("kill")
        .arg(format!("-{name}"))
        .arg(pid.to_string())
        .status();
}

fn pids() -> impl Iterator<Item = u32> {
    std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
}

// comm can contain spaces, so split after its closing paren
fn stat(pid: u32) -> Option<Vec<String>> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    Some(
        stat.rsplit_once(") ")?
            .1
            .split(' ')
            .map(String::from)
            .collect(),
    )
}

fn parent_pid(pid: u32) -> Option<u32> {
    stat(pid)?.get(1)?.parse().ok()
}

// a zombie is dead but not yet reaped
fn is_running(pid: u32) -> bool {
    stat(pid).is_some_and(|fields| fields.first().is_some_and(|state| state != "Z"))
}

fn cmdline(pid: u32) -> Option<String> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(String::from_utf8_lossy(&raw).replace('\0', " "))
}
