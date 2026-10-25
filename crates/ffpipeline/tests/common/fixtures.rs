//! Checks the property each fixture exists for, so a replaced or regenerated file can't
//! silently stop testing it. `fixtures/README.md` says why each property matters.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use super::{find_binaries, fixture_dir, mean_volume, tone_filter};

#[derive(Clone, Copy, Debug)]
pub enum Fact {
    /// codec, profile, size, SAR, pix_fmt, average frame rate
    Video,
    /// codec, sample rate, channel layout (`?` when unset); `none` when absent
    Audio,
    /// the loudest of the listed tones (Hz) in each audio channel, in channel order
    Tones,
    /// codec and size of each subtitle stream; `?x?` when probing can't find the size
    Subtitle,
    /// field order of the decoded frames: progressive, tff or bff
    Scan,
    /// range, matrix, transfer, primaries
    Color,
    /// stream side data types that must be present
    StreamSideData,
    /// frame side data types that must be present on at least one frame
    FrameSideData,
    /// Dolby Vision profile.compatibility id, plus `+el` for an enhancement layer
    DolbyVision,
    Rotation,
    Frames,
    /// keyframe times relative to the first one
    Keyframes,
    /// random access pictures by NAL type
    RandomAccess,
    /// distinct H.264 SPS levels in stream order
    Levels,
    /// whether the audio stream ends after the video stream
    AudioOutlastsVideo,
}

use Fact::*;

type Expectation = (&'static str, &'static [(Fact, &'static str)]);

const H264_1080: (Fact, &str) = (Video, "h264 High 1920x1080 1:1 yuv420p 30/1");
const HDR10: (Fact, &str) = (Color, "tv bt2020nc smpte2084 bt2020");
const HDR10_SEI: (Fact, &str) = (
    FrameSideData,
    "Mastering display metadata, Content light level metadata",
);
const LONG_GOP: (Fact, &str) = (Keyframes, "0 5.005 10.01 15.015 20.02");
const DV_VIDEO: (Fact, &str) = (Video, "hevc Main 10 1920x1080 1:1 yuv420p10le 24/1");
const H264_480: (Fact, &str) = (Video, "h264 High 640x480 1:1 yuv420p 30/1");
const SURROUND_TONES: (Fact, &str) = (Tones, "250 500 1000 60 2000 4000");
const STEREO_TONES: (Fact, &str) = (Tones, "250 500");

const FIXTURES: &[Expectation] = &[
    ("1080p_h264.ts", &[H264_1080, (Scan, "progressive")]),
    (
        "720p_h264.ts",
        &[(Video, "h264 High 1280x720 1:1 yuv420p 30/1")],
    ),
    (
        "480p_h264.ts",
        &[(Video, "h264 High 640x480 1:1 yuv420p 30/1")],
    ),
    (
        "1080p_h264_10.ts",
        &[(Video, "h264 High 10 1920x1080 1:1 yuv420p10le 24/1")],
    ),
    (
        "720p_h264_10.ts",
        &[(Video, "h264 High 10 1280x720 1:1 yuv420p10le 24/1")],
    ),
    (
        "480p_h264_10.ts",
        &[(Video, "h264 High 10 640x480 1:1 yuv420p10le 24/1")],
    ),
    (
        "1080p_hevc_10.ts",
        &[(Video, "hevc Main 10 1920x1080 1:1 yuv420p10le 24/1")],
    ),
    (
        "720p_hevc_10.ts",
        &[(Video, "hevc Main 10 1280x720 1:1 yuv420p10le 24/1")],
    ),
    (
        "480p_hevc_10.ts",
        &[(Video, "hevc Main 10 640x480 1:1 yuv420p10le 24/1")],
    ),
    (
        "480p_h264_anamorphic.ts",
        &[
            (Video, "h264 High 720x480 32:27 yuv420p 30/1"),
            (Scan, "progressive"),
        ],
    ),
    (
        "480p_h264_sps_change.ts",
        &[
            (Video, "h264 High 640x480 1:1 yuv420p 30/1"),
            (Levels, "30 51"),
            (Keyframes, "0 0.521 1.021"),
        ],
    ),
    (
        "480p_h264_a53cc.ts",
        &[(FrameSideData, "ATSC A53 Part 4 Closed Captions")],
    ),
    (
        "480p_h264_ac3_51.ts",
        &[H264_480, (Audio, "ac3 48000 5.1(side)"), SURROUND_TONES],
    ),
    (
        "480p_h264_eac3_51.mkv",
        &[H264_480, (Audio, "eac3 48000 5.1(side)"), SURROUND_TONES],
    ),
    (
        "480p_h264_dts_51.mkv",
        &[H264_480, (Audio, "dts 48000 5.1(side)"), SURROUND_TONES],
    ),
    (
        "480p_h264_truehd_51.mkv",
        &[H264_480, (Audio, "truehd 48000 5.1(side)"), SURROUND_TONES],
    ),
    (
        "480p_h264_opus_51.mkv",
        &[H264_480, (Audio, "opus 48000 5.1"), SURROUND_TONES],
    ),
    (
        "480p_h264_flac.mkv",
        &[H264_480, (Audio, "flac 48000 stereo"), STEREO_TONES],
    ),
    (
        "480p_h264_mp3.mkv",
        &[H264_480, (Audio, "mp3 48000 stereo"), STEREO_TONES],
    ),
    (
        "480p_h264_pcm_s24le.mkv",
        &[H264_480, (Audio, "pcm_s24le 48000 ?"), STEREO_TONES],
    ),
    (
        "480p_h264_video_only.mkv",
        &[
            (Video, "h264 High 640x480 1:1 yuv420p 30/1"),
            (Audio, "none"),
        ],
    ),
    (
        "720p_h264_rotated.mp4",
        &[(StreamSideData, "Display Matrix"), (Rotation, "90")],
    ),
    (
        "720p_h264_audio_longer.mkv",
        &[(Frames, "100"), (AudioOutlastsVideo, "yes")],
    ),
    (
        "480i_h264.ts",
        &[(Video, "h264 High 640x480 1:1 yuv420p 30/1"), (Scan, "bff")],
    ),
    (
        "480i_h264_anamorphic.ts",
        &[
            (Video, "h264 High 720x480 32:27 yuv420p 30/1"),
            (Scan, "bff"),
        ],
    ),
    (
        "480i_h264_motion.ts",
        &[(Video, "h264 High 640x480 1:1 yuv420p 30/1"), (Scan, "tff")],
    ),
    (
        "480i_mpeg2.ts",
        &[
            (Video, "mpeg2video Main 640x480 1:1 yuv420p 30/1"),
            (Scan, "tff"),
        ],
    ),
    ("1080i_h264.ts", &[H264_1080, (Scan, "tff")]),
    (
        "1080i_mpeg2.ts",
        &[
            (Video, "mpeg2video Main 1920x1080 1:1 yuv420p 30000/1001"),
            (Scan, "tff"),
        ],
    ),
    (
        "720p_mpeg2.ts",
        &[
            (Video, "mpeg2video Main 1280x720 1:1 yuv420p 60000/1001"),
            (Scan, "progressive"),
        ],
    ),
    (
        "480p_mpeg4.avi",
        &[(
            Video,
            "mpeg4 Advanced Simple Profile 640x480 1:1 yuv420p 30000/1001",
        )],
    ),
    (
        "1080p_vp9.webm",
        &[(Video, "vp9 Profile 0 1920x1080 1:1 yuv420p 30/1")],
    ),
    (
        "1080p_vp9_10.webm",
        &[(Video, "vp9 Profile 2 1920x1080 1:1 yuv420p10le 24/1")],
    ),
    (
        "1080p_av1.mkv",
        &[(Video, "av1 Main 1920x1080 1:1 yuv420p 30/1")],
    ),
    (
        "1080p_h264_23976.mkv",
        &[(Video, "h264 High 1920x1080 1:1 yuv420p 24000/1001")],
    ),
    (
        "720p_h264_50.ts",
        &[(Video, "h264 High 1280x720 1:1 yuv420p 50/1")],
    ),
    (
        "576i_mpeg2.ts",
        &[
            (Video, "mpeg2video Main 720x576 64:45 yuv420p 25/1"),
            (Scan, "tff"),
            (Audio, "mp2 48000 stereo"),
        ],
    ),
    (
        "1080p_h264_1916x796.mkv",
        &[(Video, "h264 High 1916x796 1:1 yuv420p 24000/1001")],
    ),
    (
        "480p_h264_sar_10x11.ts",
        &[(Video, "h264 High 720x480 10:11 yuv420p 30000/1001")],
    ),
    (
        "2160p_h264.ts",
        &[
            (Video, "h264 High 3840x2160 1:1 yuv420p 30/1"),
            (Levels, "51"),
        ],
    ),
    (
        "2160p_hevc_10_hdr.ts",
        &[
            (Video, "hevc Main 10 3840x2160 1:1 yuv420p10le 24/1"),
            HDR10,
            HDR10_SEI,
        ],
    ),
    (
        "1080p_hevc_10_hlg.ts",
        &[
            (Video, "hevc Main 10 1920x1080 1:1 yuv420p10le 24/1"),
            (Color, "tv bt2020nc arib-std-b67 bt2020"),
        ],
    ),
    (
        "1080p_hevc_10_hdr.ts",
        &[
            (Video, "hevc Main 10 1920x1080 1:1 yuv420p10le 24/1"),
            HDR10,
            HDR10_SEI,
        ],
    ),
    (
        "1080p_hevc_10_hdr_4x3.ts",
        &[
            (Video, "hevc Main 10 1440x1080 1:1 yuv420p10le 24/1"),
            HDR10,
            HDR10_SEI,
        ],
    ),
    (
        "1080p_hevc_10_hdr_scope.ts",
        &[
            (Video, "hevc Main 10 1920x800 1:1 yuv420p10le 24/1"),
            HDR10,
            HDR10_SEI,
        ],
    ),
    (
        "1080p_hevc_10_bt2020_4x3.ts",
        &[
            (Video, "hevc Main 10 1440x1080 1:1 yuv420p10le 24/1"),
            (Color, "tv bt2020nc bt2020-10 bt2020"),
        ],
    ),
    (
        "1080p_av1_10_hdr.mkv",
        &[
            (Video, "av1 Main 1920x1080 1:1 yuv420p10le 24/1"),
            HDR10,
            HDR10_SEI,
        ],
    ),
    (
        "1080p_hevc_10_dv5.mp4",
        &[
            DV_VIDEO,
            (Color, "pc ipt-c2 smpte2084 bt2020"),
            (DolbyVision, "5.0"),
        ],
    ),
    (
        "1080p_hevc_10_dv7.mp4",
        &[DV_VIDEO, HDR10, (DolbyVision, "7.6+el")],
    ),
    (
        "1080p_hevc_10_dv81.mp4",
        &[DV_VIDEO, HDR10, HDR10_SEI, (DolbyVision, "8.1")],
    ),
    (
        "1080p_hevc_10_dv82.mp4",
        &[
            DV_VIDEO,
            (Color, "tv bt709 bt709 bt709"),
            (DolbyVision, "8.2"),
        ],
    ),
    (
        "1080p_hevc_10_dv84.mp4",
        &[
            DV_VIDEO,
            (Color, "tv bt2020nc arib-std-b67 bt2020"),
            (DolbyVision, "8.4"),
        ],
    ),
    ("long_gop_h264.mp4", &[LONG_GOP, (RandomAccess, "IDR 5")]),
    ("long_gop_h264.mkv", &[LONG_GOP, (RandomAccess, "IDR 5")]),
    ("long_gop_h264.ts", &[LONG_GOP, (RandomAccess, "IDR 5")]),
    (
        "long_gop_h264_open.ts",
        &[LONG_GOP, (RandomAccess, "IDR 1")],
    ),
    (
        "long_gop_hevc_open.ts",
        &[LONG_GOP, (RandomAccess, "CRA 4, IDR 1")],
    ),
    (
        "1080p_h264_cropped_pgs.mkv",
        &[
            (Video, "h264 High 1920x1040 1:1 yuv420p 24000/1001"),
            (Subtitle, "hdmv_pgs_subtitle ?x?"),
        ],
    ),
    (
        "subtitle_pgs.sup",
        &[(Subtitle, "hdmv_pgs_subtitle 1920x1080")],
    ),
    ("subtitle.srt", &[(Subtitle, "subrip ?x?")]),
    ("subtitle.vtt", &[(Subtitle, "webvtt ?x?")]),
    ("subtitle_font.ass", &[(Subtitle, "ass ?x?")]),
    (
        "1080p_h264_subtitles.mkv",
        &[
            (Video, "h264 High 1920x1080 1:1 yuv420p 24/1"),
            (Subtitle, "subrip ?x?, ass ?x?, hdmv_pgs_subtitle 1920x1080"),
        ],
    ),
    (
        "720p_h264_mov_text.mp4",
        &[
            (Video, "h264 High 1280x720 1:1 yuv420p 24/1"),
            (Subtitle, "mov_text ?x?"),
        ],
    ),
    ("watermark.png", &[(Video, "png 200x200 1:1 rgba 25/1")]),
    (
        "watermark.jpg",
        &[(Video, "mjpeg Baseline 200x200 1:1 yuvj420p 25/1")],
    ),
    ("watermark.webp", &[(Video, "webp 200x200 ? yuva420p 25/1")]),
    (
        "watermark.gif",
        &[(Video, "gif 120x120 1:1 bgra 10/1"), (Frames, "10")],
    ),
    ("background.png", &[(Video, "png 1920x1080 1:1 pal8 25/1")]),
    (
        "background_rgb24.png",
        &[(Video, "png 1920x1080 1:1 rgb24 25/1")],
    ),
    (
        "background_alpha.png",
        &[(Video, "png 1280x720 1:1 rgba 25/1")],
    ),
];

struct Probe<'a> {
    ffmpeg: &'a Path,
    ffprobe: &'a Path,
    path: &'a Path,
    streams: Vec<Value>,
}

impl Probe<'_> {
    fn stream(&self, kind: &str) -> Option<&Value> {
        self.streams.iter().find(|s| s["codec_type"] == kind)
    }

    fn video_frames(&self) -> Vec<Value> {
        let json = run(
            self.ffprobe,
            &[
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "frame=key_frame,best_effort_timestamp_time,interlaced_frame,top_field_first:\
                 frame_side_data=side_data_type",
                "-of",
                "json",
            ],
            self.path,
        );
        let value: Value = serde_json::from_str(&json).expect("ffprobe frames json");
        value["frames"].as_array().cloned().unwrap_or_default()
    }

    fn trace_headers(&self, level: &str) -> String {
        let output = Command::new(self.ffmpeg)
            .args(["-hide_banner", "-nostdin", "-loglevel", level, "-i"])
            .arg(self.path)
            .args([
                "-map",
                "0:v:0",
                "-c",
                "copy",
                "-bsf:v",
                "trace_headers",
                "-f",
                "null",
                "-",
            ])
            .output()
            .expect("failed to run ffmpeg");
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    fn stream_end(&self, kind: &str) -> f64 {
        let selector = if kind == "video" { "v:0" } else { "a:0" };
        let csv = run(
            self.ffprobe,
            &[
                "-v",
                "error",
                "-select_streams",
                selector,
                "-show_entries",
                "packet=pts_time,duration_time",
                "-of",
                "csv=p=0",
            ],
            self.path,
        );
        csv.lines()
            .filter_map(|line| {
                let mut parts = line.split(',').map(|p| p.parse::<f64>().ok());
                Some(parts.next()?? + parts.next().flatten().unwrap_or(0.0))
            })
            .fold(0.0, f64::max)
    }

    fn tone_level(&self, channel: usize, tone: u32) -> f64 {
        let output = Command::new(self.ffmpeg)
            .args(["-hide_banner", "-nostdin", "-nostats", "-i"])
            .arg(self.path)
            .args([
                "-map",
                "0:a:0",
                "-af",
                &tone_filter(channel, tone),
                "-f",
                "null",
                "-",
            ])
            .output()
            .expect("failed to run ffmpeg");
        mean_volume(&String::from_utf8_lossy(&output.stderr)).unwrap_or(f64::NEG_INFINITY)
    }

    /// Only `Tones` reads `expected`: it lists the tones to measure.
    fn fact(&self, fact: Fact, expected: &str) -> String {
        match fact {
            Video => self.stream("video").map_or("none".into(), |s| {
                let mut parts = vec![field(s, "codec_name")];
                if let Some(profile) = s["profile"].as_str() {
                    parts.push(profile.into());
                }
                parts.push(format!("{}x{}", field(s, "width"), field(s, "height")));
                parts.push(field(s, "sample_aspect_ratio"));
                parts.push(field(s, "pix_fmt"));
                parts.push(field(s, "avg_frame_rate"));
                parts.join(" ")
            }),
            Audio => self.stream("audio").map_or("none".into(), |s| {
                format!(
                    "{} {} {}",
                    field(s, "codec_name"),
                    field(s, "sample_rate"),
                    field(s, "channel_layout")
                )
            }),
            Subtitle => {
                let subtitles: Vec<String> = self
                    .streams
                    .iter()
                    .filter(|s| s["codec_type"] == "subtitle")
                    .map(|s| {
                        format!(
                            "{} {}x{}",
                            field(s, "codec_name"),
                            field(s, "width"),
                            field(s, "height")
                        )
                    })
                    .collect();
                if subtitles.is_empty() {
                    "none".into()
                } else {
                    subtitles.join(", ")
                }
            }
            Scan => {
                let scans: BTreeSet<&str> = self
                    .video_frames()
                    .iter()
                    .map(|f| {
                        match (
                            f["interlaced_frame"].as_i64(),
                            f["top_field_first"].as_i64(),
                        ) {
                            (Some(1), Some(1)) => "tff",
                            (Some(1), _) => "bff",
                            _ => "progressive",
                        }
                    })
                    .collect();
                scans.into_iter().collect::<Vec<_>>().join("+")
            }
            Color => self.stream("video").map_or("none".into(), |s| {
                [
                    "color_range",
                    "color_space",
                    "color_transfer",
                    "color_primaries",
                ]
                .iter()
                .map(|k| field(s, k))
                .collect::<Vec<_>>()
                .join(" ")
            }),
            StreamSideData => self.stream("video").map_or("none".into(), |s| {
                side_data_types(&s["side_data_list"]).join(", ")
            }),
            FrameSideData => {
                let types: BTreeSet<String> = self
                    .video_frames()
                    .iter()
                    .flat_map(|f| side_data_types(&f["side_data_list"]))
                    .collect();
                types.into_iter().collect::<Vec<_>>().join(", ")
            }
            DolbyVision => self
                .stream("video")
                .and_then(|s| s["side_data_list"].as_array())
                .and_then(|list| list.iter().find(|d| d.get("dv_profile").is_some()))
                .map_or("none".into(), |d| {
                    let el = if d["el_present_flag"] == 1 { "+el" } else { "" };
                    format!(
                        "{}.{}{el}",
                        d["dv_profile"], d["dv_bl_signal_compatibility_id"]
                    )
                }),
            Tones => {
                let tones: Vec<u32> = expected.split(' ').filter_map(|t| t.parse().ok()).collect();
                let channels = self
                    .stream("audio")
                    .and_then(|s| s["channels"].as_u64())
                    .unwrap_or(0) as usize;
                (0..channels)
                    .map(|channel| {
                        tones
                            .iter()
                            .map(|&tone| (tone, self.tone_level(channel, tone)))
                            .max_by(|a, b| a.1.total_cmp(&b.1))
                            .map_or("?".into(), |(tone, _)| tone.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            }
            Rotation => self
                .stream("video")
                .and_then(|s| s["side_data_list"].as_array())
                .and_then(|list| list.iter().find_map(|d| d.get("rotation")))
                .map_or("none".into(), Value::to_string),
            Frames => self.video_frames().len().to_string(),
            Keyframes => {
                let times: Vec<f64> = self
                    .video_frames()
                    .iter()
                    .filter(|f| f["key_frame"] == 1)
                    .filter_map(|f| f["best_effort_timestamp_time"].as_str()?.parse().ok())
                    .collect();
                times
                    .iter()
                    .map(|t| format!("{}", ((t - times[0]) * 1000.0).round() / 1000.0))
                    .collect::<Vec<_>>()
                    .join(" ")
            }
            RandomAccess => {
                // count key packets only: ffmpeg traces some packets twice, and the TS
                // demuxer can split a CRA access unit across two packets
                let mut pictures: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
                let mut pts = String::new();
                for line in self.trace_headers("debug").lines() {
                    if let Some(rest) = line.split("Packet: ").nth(1) {
                        pts = if rest.contains("key frame") {
                            rest.split(", ")
                                .find(|p| p.starts_with("pts "))
                                .unwrap_or("")
                        } else {
                            ""
                        }
                        .into();
                        continue;
                    }
                    // e.g. "nal_unit_type: 5(IDR), ..."
                    let Some(rest) = line.split("nal_unit_type: ").nth(1) else {
                        continue;
                    };
                    let hevc = line.contains("nuh_layer_id");
                    let name = match rest.split('(').next().and_then(|n| n.parse::<u8>().ok()) {
                        Some(5) if !hevc => "IDR",
                        Some(19 | 20) if hevc => "IDR",
                        Some(21) if hevc => "CRA",
                        _ => continue,
                    };
                    if !pts.is_empty() {
                        pictures.entry(name).or_default().insert(pts.clone());
                    }
                }
                pictures
                    .iter()
                    .map(|(name, pts)| format!("{name} {}", pts.len()))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
            Levels => {
                let mut levels: Vec<String> = Vec::new();
                for line in self.trace_headers("debug").lines() {
                    if line.contains(" level_idc ") {
                        let level = line.rsplit(' ').next().unwrap_or_default().to_string();
                        if levels.last() != Some(&level) {
                            levels.push(level);
                        }
                    }
                }
                levels.join(" ")
            }
            AudioOutlastsVideo => {
                if self.stream_end("audio") > self.stream_end("video") {
                    "yes".into()
                } else {
                    "no".into()
                }
            }
        }
    }
}

fn run(program: &Path, args: &[&str], path: &Path) -> String {
    let output = Command::new(program)
        .args(args)
        .arg(path)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", program.display()));
    assert!(
        output.status.success(),
        "{} {} failed: {}",
        program.display(),
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn field(stream: &Value, key: &str) -> String {
    match &stream[key] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => "?".into(),
    }
}

fn side_data_types(list: &Value) -> Vec<String> {
    list.as_array()
        .map(|l| {
            l.iter()
                .filter_map(|d| d["side_data_type"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Side data facts list what must be present; others must match exactly.
fn matches(fact: Fact, actual: &str, expected: &str) -> bool {
    match fact {
        StreamSideData | FrameSideData => {
            let actual: Vec<&str> = actual.split(", ").collect();
            expected.split(", ").all(|e| actual.contains(&e))
        }
        _ => actual == expected,
    }
}

pub fn check_all() {
    let (ffmpeg, ffprobe) = find_binaries().expect("ffmpeg and ffprobe are required");
    let dir = fixture_dir();
    let mut failures = Vec::new();

    let on_disk: BTreeSet<String> = std::fs::read_dir(&dir)
        .expect("fixture dir")
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "README.md")
        .collect();
    let listed: BTreeSet<String> = FIXTURES.iter().map(|(n, _)| n.to_string()).collect();
    for name in on_disk.difference(&listed) {
        failures.push(format!(
            "{name}: not listed in common/fixtures.rs (document it in fixtures/README.md)"
        ));
    }
    for name in listed.difference(&on_disk) {
        failures.push(format!("{name}: listed but missing from {}", dir.display()));
    }

    for (name, facts) in FIXTURES {
        let path = dir.join(name);
        if path.exists() {
            failures.extend(check_file(&ffmpeg, &ffprobe, &path, facts.iter().copied()));
        }
    }

    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Fact names as written in the external media manifest.
pub fn parse_fact(name: &str) -> Option<Fact> {
    [
        Video,
        Audio,
        Tones,
        Subtitle,
        Scan,
        Color,
        StreamSideData,
        FrameSideData,
        DolbyVision,
        Rotation,
        Frames,
        Keyframes,
        RandomAccess,
        Levels,
        AudioOutlastsVideo,
    ]
    .into_iter()
    .find(|fact| format!("{fact:?}") == name)
}

pub fn check_file<'a>(
    ffmpeg: &Path,
    ffprobe: &Path,
    path: &Path,
    facts: impl IntoIterator<Item = (Fact, &'a str)>,
) -> Vec<String> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let streams = run(
        ffprobe,
        &["-v", "error", "-show_streams", "-of", "json"],
        path,
    );
    let streams: Value = serde_json::from_str(&streams).expect("ffprobe streams json");
    let probe = Probe {
        ffmpeg,
        ffprobe,
        path,
        streams: streams["streams"].as_array().cloned().unwrap_or_default(),
    };
    facts
        .into_iter()
        .filter_map(|(fact, expected)| {
            let actual = probe.fact(fact, expected);
            (!matches(fact, &actual, expected))
                .then(|| format!("{name}: {fact:?} expected \"{expected}\", got \"{actual}\""))
        })
        .collect()
}
