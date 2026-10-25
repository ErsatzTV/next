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
use std::time::Duration;

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
        let is_doc = name == "manifest.json" || name.ends_with(".md");
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

/// One test for all entries, so a manifest change needs no code change. Each check runs in
/// its own task so one failure doesn't stop the rest.
pub async fn run_all(accel: Option<HardwareAccel>) {
    let Some(dir) = media_dir() else {
        return;
    };
    let mut failures = Vec::new();
    for entry in manifest(&dir).media {
        // `TestCase` names its source with a `'static` str; an absolute path replaces the
        // fixture folder when joined
        let path: &'static str = dir.join(&entry.file).to_string_lossy().into_owned().leak();
        for check in &entry.checks {
            let label = format!("{} [{check}]", entry.file);
            let Some(mut test_case) = test_case(path, check) else {
                failures.push(format!("{label}: unknown check"));
                continue;
            };
            test_case.params.in_point = Duration::from_millis(entry.in_point_ms);
            // film grain fails the ssim check at the fixtures' 5 Mb/s; picture correctness,
            // not rate control, is what this checks
            test_case.params.video_bitrate = Some(Kbps(20000));
            test_case.params.video_buffer = Some(Kbps(40000));
            if let Err(e) = tokio::spawn(shared::run(accel.clone(), test_case)).await {
                let message = e
                    .try_into_panic()
                    .ok()
                    .and_then(|p| {
                        p.downcast_ref::<String>()
                            .cloned()
                            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                    })
                    .unwrap_or_else(|| String::from("cancelled"));
                failures.push(format!("{label}: {message}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} external media checks failed:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
