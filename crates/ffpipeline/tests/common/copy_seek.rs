// only the software suite uses this, but every suite compiles `common`
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ffpipeline::copy_decision::{CopyBlocker, CopyDecision, CopyDecisions};
use ffpipeline::keyframe_seek::{CopySeekPlan, CopySeekRequest, KeyframeLocator, plan_copy_seek};
use ffpipeline::output_settings::CopyPolicy;
use ffpipeline::pipeline::{PtsOffset, generate_pipeline};

use super::*;

/// Fixtures are 24 s, with a keyframe every 5.005 s.
const ITEM_END: Duration = Duration::from_secs(24);
/// Short enough that the item splits into two chunks
const CHUNK_LIMIT: Duration = Duration::from_secs(12);

pub struct CopySeekCase {
    pub fixture_name: &'static str,
    pub join: Duration,
    /// An open GOP's leading B-frames reference the previous GOP and can't decode after a seek.
    pub open_gop: bool,
}

pub async fn run_copy_seek_test(case: CopySeekCase) {
    let Some(env) = test_env().await else { return };
    let source = fixture_path(case.fixture_name);
    let probe = probe_file(&env.ffmpeg, &env.ffprobe, &source).await;
    let frame_rate = probe_avg_frame_rate(&env.ffprobe, &source).await;
    let frame = Duration::from_secs_f64(1.0 / frame_rate.parsed_frame_rate);
    let (video_index, audio_index) = stream_indexes(&probe);

    let dir = tempfile::tempdir().unwrap();
    let scheduled = ITEM_END - case.join;
    let mut target = case.join;
    let mut remaining = scheduled;
    let mut resume = None;
    let mut pts_offset = None;
    let mut chunks: Vec<(PathBuf, CopySeekPlan)> = Vec::new();

    for (n, limit) in [Some(CHUNK_LIMIT), None].into_iter().enumerate() {
        let chunk_dir = dir.path().join(format!("chunk{n}"));
        std::fs::create_dir(&chunk_dir).unwrap();

        let mut input = build_input(&source, probe.clone(), Duration::ZERO, None);
        let plan = {
            let locator =
                KeyframeLocator::new(&env.ffmpeg, &input.video_input, video_index, audio_index);
            plan_copy_seek(
                &locator,
                CopySeekRequest {
                    target,
                    floor: Duration::ZERO,
                    remaining,
                    limit,
                    resume,
                },
            )
            .await
            .unwrap()
        };
        log::info!("chunk {n}: {plan:?}");
        for probed in [&mut input.video_input, &mut input.audio_input] {
            probed.in_point = plan.in_point;
            probed.out_point = plan.in_point + plan.duration;
        }
        input.video_copy_seek = Some(plan.seek);

        let mut output = build_output(
            &chunk_dir,
            TestOutputParams {
                video_copy: Some(CopyPolicy::default()),
                audio_copy: Some(CopyPolicy::default()),
                ..TestOutputParams::default()
            },
        );
        output.pts_offset = pts_offset;

        let mut pipeline = generate_pipeline(&env.ffmpeg_info, input, output).unwrap();
        assert_eq!(
            pipeline.copy_decisions(),
            &CopyDecisions {
                video: Some(CopyDecision::Copy),
                audio: Some(CopyDecision::Copy),
            }
        );
        pipeline.optimize();
        let (success, stderr) = run_ffmpeg_pipeline(&env.ffmpeg, &pipeline).await;
        assert!(success, "chunk {n} failed: {stderr}");

        pts_offset = Some(PtsOffset {
            duration: last_segment_end(&env.ffprobe, &chunk_dir).await,
        });
        target += plan.duration;
        remaining -= plan.duration;
        resume = plan.seek.end;
        chunks.push((chunk_dir, plan));
    }

    let (first_plan, last_plan) = (chunks[0].1, chunks[1].1);
    assert!(
        !first_plan.is_complete && first_plan.seek.end.is_some(),
        "the join chunk should end on a keyframe: {first_plan:?}"
    );
    assert!(last_plan.is_complete);
    assert!(
        first_plan.in_point <= case.join && case.join - first_plan.in_point < frame * 150,
        "the join should shift back less than one GOP: {first_plan:?}"
    );

    let total: f64 = chunks.iter().map(|(dir, _)| extinf_total(dir)).sum();
    assert!(
        (total - scheduled.as_secs_f64()).abs() <= 3.0 * frame.as_secs_f64(),
        "HLS total {total:.3}s should match the scheduled {:.3}s",
        scheduled.as_secs_f64()
    );

    let boundary = last_segment_end(&env.ffprobe, &chunks[0].0).await;
    let resumed_at = first_video_pts(&env.ffprobe, &first_segment(&chunks[1].0)).await;
    assert!(
        resumed_at.abs_diff(boundary) <= frame / 2,
        "chunk 1 starts at {resumed_at:?}, chunk 0 ends at {boundary:?}"
    );

    let source_frames: HashMap<String, usize> = framemd5(&env.ffmpeg, &source)
        .await
        .into_iter()
        .enumerate()
        .map(|(i, hash)| (hash, i))
        .collect();
    let mut matched = Vec::new();
    for (chunk_dir, _) in &chunks {
        let joined = chunk_dir.join("all.ts");
        let mut bytes = Vec::new();
        for segment in segments(chunk_dir) {
            bytes.extend(std::fs::read(segment).unwrap());
        }
        std::fs::write(&joined, bytes).unwrap();
        let frames = framemd5(&env.ffmpeg, &joined).await;
        let unmatched = frames
            .iter()
            .filter(|h| !source_frames.contains_key(*h))
            .count();
        assert_eq!(unmatched, 0, "output frames that aren't source frames");
        matched.push(
            frames
                .iter()
                .map(|h| source_frames[h])
                .collect::<Vec<usize>>(),
        );
    }

    // ts audio often starts before the video
    let first_frame = {
        let input = build_input(&source, probe.clone(), Duration::ZERO, None);
        let locator =
            KeyframeLocator::new(&env.ffmpeg, &input.video_input, video_index, audio_index);
        locator.land(Duration::ZERO).await.unwrap().unwrap().time
    };
    let first_index =
        ((first_plan.in_point - first_frame).as_secs_f64() / frame.as_secs_f64()).round() as usize;
    assert_eq!(
        matched[0][0], first_index,
        "chunk 0 must start on the keyframe"
    );
    // `-t` stops on dts, so the item end can drop a reordered B-frame
    let tail = matched[1].len().saturating_sub(4);
    let checked: Vec<usize> = matched[0]
        .iter()
        .chain(&matched[1][..tail])
        .copied()
        .collect();
    let max_skipped = if case.open_gop { 3 } else { 0 };
    for (n, pair) in checked.windows(2).enumerate() {
        let at_boundary = n + 1 == matched[0].len();
        assert!(
            pair[1] > pair[0],
            "frame {} repeated or out of order after {}",
            pair[1],
            pair[0]
        );
        assert!(
            pair[1] - pair[0] - 1 <= if at_boundary { max_skipped } else { 0 },
            "skipped frames {}..{}",
            pair[0] + 1,
            pair[1]
        );
    }
    let all: Vec<usize> = matched.concat();
    let end_index = matched[0].last().unwrap();
    let resume_index = matched[1][0];
    log::info!(
        "{}: frames {}..={end_index} then {resume_index}..={}, HLS total {total:.3}s",
        case.fixture_name,
        matched[0][0],
        all.last().unwrap()
    );
}

/// Cuts at 188-byte packet boundaries, like a DVR or commercial cutter.
fn byte_cut_ts(source: &Path, out: &Path, from: Duration, length: Duration, total: Duration) {
    let bytes = std::fs::read(source).unwrap();
    let at = |t: Duration| {
        let offset = (bytes.len() as f64 * t.as_secs_f64() / total.as_secs_f64()) as usize;
        offset / 188 * 188
    };
    let start = at(from);
    std::fs::write(out, &bytes[start..(start + at(length)).min(bytes.len())]).unwrap();
}

pub async fn run_input_start_test() {
    let Some(env) = test_env().await else { return };
    let dir = tempfile::tempdir().unwrap();
    let ts = fixture_path("long_gop_h264.ts");
    let mid_gop = dir.path().join("mid_gop.ts");
    let no_keyframe = dir.path().join("no_keyframe.ts");
    // keyframes at 0, 5.005, 10.01 s: one cut starts 2 s into a GOP, one has no keyframe
    byte_cut_ts(
        &ts,
        &mid_gop,
        Duration::from_secs(7),
        Duration::from_secs(10),
        ITEM_END,
    );
    byte_cut_ts(
        &ts,
        &no_keyframe,
        Duration::from_millis(11_000),
        Duration::from_millis(3_000),
        ITEM_END,
    );

    let mut cases: Vec<(PathBuf, Option<CopyBlocker>)> = [
        "long_gop_h264.mp4",
        "long_gop_h264.mkv",
        "long_gop_h264.ts",
        "long_gop_h264_open.ts",
        "long_gop_hevc_open.ts",
    ]
    .into_iter()
    .map(|f| (fixture_path(f), None))
    .collect();
    cases.push((mid_gop, Some(CopyBlocker::StartsBetweenKeyframes)));
    cases.push((no_keyframe, Some(CopyBlocker::NoKeyframes)));

    for (path, expected) in cases {
        let probe = probe_file(&env.ffmpeg, &env.ffprobe, &path).await;
        let (video_index, audio_index) = stream_indexes(&probe);
        let input = build_input(&path, probe, Duration::ZERO, None);
        let locator =
            KeyframeLocator::new(&env.ffmpeg, &input.video_input, video_index, audio_index);
        let start = locator.input_start().await.unwrap();
        log::info!("{}: {start:?}", path.display());
        assert_eq!(
            start.map_or(Some(CopyBlocker::NoKeyframes), |s| s.copy_blocker()),
            expected,
            "{}",
            path.display()
        );
        if expected == Some(CopyBlocker::StartsBetweenKeyframes) {
            // the next keyframe is about 3 s in
            let keyframe = start.unwrap().first_keyframe.unwrap();
            assert!(
                keyframe.time > Duration::from_secs(1) && keyframe.time < Duration::from_secs(5)
            );
        }
    }
}

fn stream_indexes(probe: &ProbeResult) -> (u32, Option<u32>) {
    let video = probe
        .streams
        .iter()
        .find_map(|s| match s {
            ProbeResultStream::Video(v) => Some(v.stream_index),
            _ => None,
        })
        .unwrap();
    let audio = probe.streams.iter().find_map(|s| match s {
        ProbeResultStream::Audio(a) => Some(a.stream_index),
        _ => None,
    });
    (video, audio)
}

fn segments(dir: &Path) -> Vec<PathBuf> {
    let mut segments: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("segment_"))
        })
        .collect();
    segments.sort();
    segments
}

fn first_segment(dir: &Path) -> PathBuf {
    segments(dir).into_iter().next().expect("no segments")
}

fn extinf_total(dir: &Path) -> f64 {
    std::fs::read_to_string(dir.join("live.m3u8"))
        .unwrap()
        .lines()
        .filter_map(|l| l.strip_prefix("#EXTINF:"))
        .map(|l| l.trim_end_matches(',').parse::<f64>().unwrap())
        .sum()
}

/// Matches `PtsScanner` in ersatztv-channel.
async fn last_segment_end(ffprobe: &Path, dir: &Path) -> Duration {
    let segment = segments(dir).pop().expect("no segments");
    let output = ersatztv_core::process::command(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "packet=pts_time,duration_time",
        ])
        .args(["-of", "compact=p=0:nk=1"])
        .arg(&segment)
        .output()
        .await
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.trim().split('|');
            let pts = fields.next()?.parse::<f64>().ok()?;
            let duration = fields.next().and_then(|d| d.parse::<f64>().ok());
            Some(pts + duration.unwrap_or(0.0))
        })
        .map(Duration::from_secs_f64)
        .max()
        .unwrap_or_default()
}

/// First in decode order, so an open GOP's leading B-frames don't count.
async fn first_video_pts(ffprobe: &Path, segment: &Path) -> Duration {
    let output = ersatztv_core::process::command(ffprobe)
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-show_entries", "packet=pts_time", "-of", "csv=p=0"])
        .arg(segment)
        .output()
        .await
        .unwrap();
    let first = String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|l| l.trim().trim_end_matches(',').parse::<f64>().ok())
        .expect("no video packets");
    Duration::from_secs_f64(first)
}

async fn framemd5(ffmpeg: &Path, path: &Path) -> Vec<String> {
    let output = ersatztv_core::process::command(ffmpeg)
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-f", "framemd5", "-"])
        .output()
        .await
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.rsplit(',').next().map(|h| h.trim().to_owned()))
        .collect()
}
