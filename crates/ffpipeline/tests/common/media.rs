//! Real media that can't be committed (copyright, size), listed in
//! `$ETV_TEST_MEDIA/manifest.json`. Without `ETV_TEST_MEDIA` these tests pass without
//! running anything.
//!
//! ```json
//! { "media": [{ "file": "clip.mkv", "role": "why it's here", "in_point_ms": 2000,
//!   "facts": { "Video": "vc1 Advanced 1920x1080 1:1 yuv420p 24000/1001" },
//!   "checks": ["transcode", "tonemap", "deinterlace", "subtitle:2"] }] }
//! ```
//!
//! `facts` use the `fixtures.rs` names and formats; `subtitle:<n>` burns embedded stream `n`.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use ffpipeline::frame_size::FrameSize;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::pipeline::{AudioFormat, Kbps};
use serde::Deserialize;

use super::{TestCase, find_binaries, fixtures, shared};

#[derive(Deserialize)]
struct Manifest {
    media: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    file: String,
    #[serde(default)]
    in_point_ms: u64,
    #[serde(default)]
    facts: BTreeMap<String, String>,
    checks: Vec<String>,
}

const SIZE_1080P: FrameSize = FrameSize {
    width: 1920,
    height: 1080,
};

fn media_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("ETV_TEST_MEDIA").map(PathBuf::from);
    if dir.is_none() {
        log::warn!("ETV_TEST_MEDIA is not set; skipping external media");
    }
    dir
}

fn manifest(dir: &std::path::Path) -> Manifest {
    let path = dir.join("manifest.json");
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("can't read {}: {e}", path.display()));
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("bad {}: {e}", path.display()))
}

/// Every file is listed and still has the listed properties, so a replaced file is caught.
pub fn check_properties() {
    let Some(dir) = media_dir() else {
        return;
    };
    let manifest = manifest(&dir);
    let (ffmpeg, ffprobe) = find_binaries().expect("ffmpeg and ffprobe are required");
    let mut failures = Vec::new();

    let listed: BTreeSet<&str> = manifest.media.iter().map(|e| e.file.as_str()).collect();
    for entry in std::fs::read_dir(&dir)
        .expect("media dir")
        .filter_map(Result::ok)
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_doc = name == "manifest.json" || name == "SHA256SUMS" || name.ends_with(".md");
        if entry.path().is_file() && !is_doc && !listed.contains(name.as_str()) {
            failures.push(format!("{name}: not listed in manifest.json"));
        }
    }

    for entry in &manifest.media {
        let path = dir.join(&entry.file);
        if !path.exists() {
            failures.push(format!("{}: listed but missing", entry.file));
            continue;
        }
        let mut facts = Vec::new();
        for (name, expected) in &entry.facts {
            match fixtures::parse_fact(name) {
                Some(fact) => facts.push((fact, expected.as_str())),
                None => failures.push(format!("{}: unknown fact {name}", entry.file)),
            }
        }
        failures.extend(fixtures::check_file(&ffmpeg, &ffprobe, &path, facts));
    }

    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn test_case(path: &'static str, check: &str) -> Option<TestCase> {
    let vf = ("h264", 8);
    let af = AudioFormat::Aac;
    Some(match check {
        "transcode" => shared::transcode(path, SIZE_1080P, vf, af),
        "tonemap" => shared::tonemap(path, SIZE_1080P, vf, af, false),
        "deinterlace" => shared::deinterlace(path, SIZE_1080P, vf, af, true),
        _ => {
            let stream = check.strip_prefix("subtitle:")?.parse().ok()?;
            shared::embedded_subtitle(path, stream, None, None)
        }
    })
}

tokio::task_local! {
    static CAPTURED: Arc<Mutex<Vec<String>>>;
}

/// False outside a check, so the caller logs as usual.
pub(super) fn capture(line: String) -> bool {
    CAPTURED
        .try_with(|lines| lines.lock().unwrap().push(line))
        .is_ok()
}

/// The summary reports the message; the default hook would print it again.
fn quiet_check_panics() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if CAPTURED.try_with(|_| ()).is_err() {
                default(info);
            }
        }));
    });
}

const MAX_DETAIL_LINES: usize = 40;

/// The address in `[h264 @ 0x55d0…]` (`@ 000000f0…` on Windows) differs per thread, so
/// repeats would not fold.
fn strip_pointers(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find(" @ ") {
        let after = &rest[at + 3..];
        let digits = after.strip_prefix("0x").unwrap_or(after);
        let hex = digits.len()
            - digits
                .trim_start_matches(|c: char| c.is_ascii_hexdigit())
                .len();
        if hex > 0 && digits[hex..].starts_with(']') {
            out.push_str(&rest[..at]);
            rest = &digits[hex..];
        } else {
            out.push_str(&rest[..at + 3]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// ffmpeg repeats lines per frame; show each once with a count.
fn fold_repeats(text: &str) -> String {
    let mut lines: Vec<(String, usize)> = Vec::new();
    for line in text.lines().map(strip_pointers) {
        match lines.iter_mut().find(|(seen, _)| *seen == line) {
            Some((_, count)) => *count += 1,
            None => lines.push((line, 1)),
        }
    }
    let skipped = lines.len().saturating_sub(MAX_DETAIL_LINES);
    let mut out = Vec::new();
    if skipped > 0 {
        out.push(format!("... {skipped} earlier lines omitted"));
    }
    for (line, count) in &lines[skipped..] {
        out.push(match count {
            1 => line.clone(),
            n => format!("{line} (x{n})"),
        });
    }
    out.join("\n")
}

/// Adds the next line when the first only introduces it ("ffmpeg failed:").
fn headline(message: &str) -> String {
    let mut lines = message.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or_default();
    let line = match lines.next() {
        Some(next) if first.ends_with(':') => format!("{first} {next}"),
        _ => first.to_string(),
    };
    strip_pointers(&line)
}

fn panic_message(e: tokio::task::JoinError) -> String {
    e.try_into_panic()
        .ok()
        .and_then(|p| {
            p.downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| String::from("cancelled"))
}

struct Failure {
    label: String,
    message: String,
    log: Vec<String>,
}

/// One test for all entries, so a manifest change needs no code change. Each check runs in
/// its own task so one failure doesn't stop the rest. `ETV_TEST_MEDIA_FILTER` runs only
/// checks whose `file [check]` label contains it.
pub async fn run_all(accel: Option<HardwareAccel>) {
    let Some(dir) = media_dir() else {
        return;
    };
    quiet_check_panics();
    // libtest's "test external_media ... " has no newline
    println!();
    let suite = env!("CARGO_CRATE_NAME");
    let filter = std::env::var("ETV_TEST_MEDIA_FILTER").ok();
    let mut passed = 0;
    let mut filtered = 0;
    let mut failures = Vec::new();
    for entry in manifest(&dir).media {
        // `TestCase` names its source with a `'static` str; an absolute path replaces the
        // fixture folder when joined
        let path: &'static str = dir.join(&entry.file).to_string_lossy().into_owned().leak();
        for check in &entry.checks {
            let label = format!("{} [{check}]", entry.file);
            if filter.as_ref().is_some_and(|f| !label.contains(f.as_str())) {
                filtered += 1;
                continue;
            }
            let Some(mut test_case) = test_case(path, check) else {
                println!("FAIL         {label}: unknown check");
                failures.push(Failure {
                    label,
                    message: String::from("unknown check"),
                    log: Vec::new(),
                });
                continue;
            };
            test_case.params.in_point = Duration::from_millis(entry.in_point_ms);
            // a mid-GOP capture holds its first picture for up to 2 s
            test_case.params.duration = Duration::from_secs(4);
            // film grain fails the ssim check at the fixtures' 5 Mb/s; picture correctness,
            // not rate control, is what this checks
            test_case.params.video_bitrate = Some(Kbps(20000));
            test_case.params.video_buffer = Some(Kbps(40000));

            let log = Arc::new(Mutex::new(Vec::new()));
            let started = Instant::now();
            let result =
                tokio::spawn(CAPTURED.scope(log.clone(), shared::run(accel.clone(), test_case)))
                    .await;
            let secs = started.elapsed().as_secs_f64();
            match result {
                Ok(()) => {
                    passed += 1;
                    println!("PASS  {secs:5.1}s  {label}");
                }
                Err(e) => {
                    let message = panic_message(e);
                    println!("FAIL  {secs:5.1}s  {label}: {}", headline(&message));
                    let log = std::mem::take(&mut *log.lock().unwrap());
                    failures.push(Failure {
                        label,
                        message,
                        log,
                    });
                }
            }
        }
    }

    let mut summary = format!(
        "{suite} external_media: {passed} passed, {} failed",
        failures.len()
    );
    if filtered > 0 {
        summary.push_str(&format!(", {filtered} filtered out"));
    }
    assert!(
        passed + failures.len() > 0,
        "ETV_TEST_MEDIA_FILTER={filter:?} matched no checks"
    );
    println!("{summary}");
    if failures.is_empty() {
        return;
    }
    for f in &failures {
        println!("\n--- {}\n{}", f.label, fold_repeats(&f.message));
        for line in &f.log {
            println!("log: {line}");
        }
        println!(
            "rerun: ETV_TEST_MEDIA_FILTER=\"{}\" cargo test -p ffpipeline --test {suite} -- \
             --ignored --nocapture external_media",
            f.label
        );
    }
    // skips the panic hook, whose stderr copy of the summary can land mid-output in CI
    std::panic::resume_unwind(Box::new(summary));
}
