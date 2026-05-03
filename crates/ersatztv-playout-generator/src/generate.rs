use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ersatztv_playout::playout::{
    DATE_FORMAT, Playout, PlayoutItem, PlayoutItemSource, PlayoutItemTracks, TrackSelection,
    parse_playout_filename,
};
use ffpipeline::probe::{ProbeResult, ProbeResultStream};
use rand::RngExt;
use rand::prelude::SliceRandom;
use time::OffsetDateTime;
use walkdir::WalkDir;

use crate::error::PlayoutGeneratorError;
use crate::{IMAGE_EXTENSIONS, is_image_extension, is_video_extension, to_probe_result};

const BUILD_UNDER: time::Duration = time::Duration::hours(36);
const TO_BUILD: time::Duration = time::Duration::hours(48);
const TO_RETAIN: time::Duration = time::Duration::days(1);

pub async fn generate_playout(
    content_folder: &Path,
    output_folder: &PathBuf,
) -> Result<(), PlayoutGeneratorError> {
    let video_list = probe_content(content_folder).await?;
    if video_list.is_empty() {
        return Err(PlayoutGeneratorError::NoSourceContent);
    }

    if !output_folder.exists() {
        tokio::fs::create_dir_all(output_folder).await?;
    }

    let now = OffsetDateTime::now_local()?;
    let threshold = now + BUILD_UNDER;
    let target = now + TO_BUILD;
    let horizon = scan_horizon(output_folder).await?;

    if let Some(h) = horizon
        && h >= threshold
    {
        log::info!("nothing to add before {target}");
        return Ok(());
    }

    let gen_start = horizon.filter(|h| *h > now).unwrap_or(now);

    let mut rng = rand::rng();
    let playout_items = build_items(gen_start, target, &video_list, &mut rng).await;

    write_playout_file(output_folder, playout_items).await?;

    gc_old_playouts(output_folder, now - TO_RETAIN).await?;

    Ok(())
}

async fn probe_content(
    content_folder: &Path,
) -> Result<Vec<(PathBuf, ProbeResult)>, PlayoutGeneratorError> {
    // find all available video files
    let mut video_paths: HashMap<PathBuf, ProbeResult> = HashMap::new();
    for dir_entry in WalkDir::new(content_folder)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| is_video_extension(e) || is_image_extension(e))
    {
        if let Some(path_and_probe) = to_probe_result(&dir_entry).await {
            log::debug!("path: {path_and_probe:?}");
            video_paths.insert(path_and_probe.path, path_and_probe.probe);
        }
    }

    Ok(video_paths.into_iter().collect())
}

async fn scan_horizon(
    output_folder: &Path,
) -> Result<Option<OffsetDateTime>, PlayoutGeneratorError> {
    let mut result = None;

    let mut entries = tokio::fs::read_dir(output_folder)
        .await
        .map_err(PlayoutGeneratorError::IoFailure)?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Some(file_name_os) = entry.path().file_stem() {
            let file_name = file_name_os.to_string_lossy();

            if let Some((_, finish)) = parse_playout_filename(&file_name)
                && (result.is_none() || result.is_some_and(|max| finish > max))
            {
                result = Some(finish);
            }
        }
    }

    Ok(result)
}

async fn build_items(
    start: OffsetDateTime,
    finish: OffsetDateTime,
    video_list: &[(PathBuf, ProbeResult)],
    rng: &mut impl rand::Rng,
) -> Vec<PlayoutItem> {
    let mut playout_items: Vec<PlayoutItem> = Vec::new();
    let mut current_time = start;
    let mut shuffled = video_list.to_vec();
    shuffled.shuffle(rng);
    let mut cursor = 0;

    while current_time < finish {
        if cursor >= shuffled.len() {
            let last = shuffled.last().cloned();
            shuffled.shuffle(rng);
            if shuffled.len() > 1
                && let Some(ref last) = last
                && shuffled[0].0 == last.0
            {
                let swap_index = rng.random_range(1..shuffled.len());
                shuffled.swap(0, swap_index);
            }
            cursor = 0;
        }

        let (path, probe_result) = &shuffled[cursor];
        cursor += 1;

        let is_image = probe_result.streams.len() == 1
            && matches!(probe_result.streams.first(), Some(ProbeResultStream::Video(video_stream)) if IMAGE_EXTENSIONS.contains(&video_stream.codec.as_str()));

        let has_audio = probe_result
            .streams
            .iter()
            .any(|s| matches!(s, ProbeResultStream::Audio(_)));

        let has_image_subtitle = probe_result
            .streams
            .iter()
            .any(|s| matches!(s, ProbeResultStream::Video(video_stream) if video_stream.is_subtitle_image()));

        // use 10-sec duration for images
        let image_duration = std::time::Duration::from_secs(10);
        let duration = match probe_result.duration {
            Some(d) => Some(d),
            None => {
                if is_image {
                    Some(image_duration)
                } else {
                    None
                }
            }
        };

        if let Some(scheduled_duration) = duration
            && let Ok(mut playout_item) = PlayoutItem::new(
                uuid::Uuid::new_v4().to_string(),
                current_time,
                current_time + scheduled_duration,
                None,
                None,
                path,
            )
        {
            // use separate tracks with lavfi audio for images
            if !has_audio {
                playout_item.tracks = Some(PlayoutItemTracks {
                    audio: Some(TrackSelection {
                        source: Some(PlayoutItemSource::Lavfi {
                            params: String::from(
                                "anullsrc=channel_layout=stereo:sample_rate=48000",
                            ),
                        }),
                        stream_index: None,
                    }),
                    video: playout_item.source.as_ref().map(|s| TrackSelection {
                        source: Some(s.clone()),
                        stream_index: None,
                    }),
                    subtitle: None,
                });

                playout_item.source = None;
            }

            if has_image_subtitle && playout_item.tracks.is_none() {
                for stream in probe_result.streams.iter() {
                    if let ProbeResultStream::Video(video_stream) = stream
                        && video_stream.is_subtitle_image()
                    {
                        playout_item.tracks = Some(PlayoutItemTracks {
                            audio: None,
                            video: None,
                            subtitle: Some(TrackSelection {
                                source: None,
                                stream_index: Some(video_stream.stream_index),
                            }),
                        })
                    }
                }
            }

            if !has_image_subtitle
                && playout_item.tracks.is_none()
                && let (Some(parent), Some(stem)) =
                    (path.parent(), path.file_stem().and_then(|s| s.to_str()))
                && let Ok(mut entries) = tokio::fs::read_dir(parent).await
            {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let entry_path = entry.path();

                    if entry_path.is_file()
                        && let (Some(name), Some(ext)) = (
                            entry_path.file_name().and_then(|n| n.to_str()),
                            entry_path.extension().and_then(|e| e.to_str()),
                        )
                        && name.starts_with(stem)
                        && ext == "srt"
                    {
                        playout_item.tracks = Some(PlayoutItemTracks {
                            audio: None,
                            video: None,
                            subtitle: Some(TrackSelection {
                                source: Some(PlayoutItemSource::Local {
                                    path: entry_path.to_string_lossy().to_string(),
                                    in_point_ms: None,
                                    out_point_ms: None,
                                }),
                                stream_index: None,
                            }),
                        });
                        break;
                    }
                }
            }

            playout_items.push(playout_item);
            current_time += scheduled_duration;
        }
    }

    playout_items
}

async fn write_playout_file(
    output_folder: &Path,
    items: Vec<PlayoutItem>,
) -> Result<(), PlayoutGeneratorError> {
    if let Some(first) = items.first()
        && let Some(last) = items.last()
    {
        // generate output file name
        let formatted_start = first.start.format(&DATE_FORMAT)?;
        let formatted_finish = last.finish.format(&DATE_FORMAT)?;

        let output_file = format!("{formatted_start}_{formatted_finish}.json");
        log::info!("output_file: {output_file}");

        let output_path = output_folder.join(&output_file);
        let playout = Playout::new(items);
        let output_string = serde_json::to_string_pretty(&playout)?;
        tokio::fs::write(&output_path, output_string).await?;
    }

    Ok(())
}

async fn gc_old_playouts(
    output_folder: &Path,
    before: OffsetDateTime,
) -> Result<(), PlayoutGeneratorError> {
    let mut entries = tokio::fs::read_dir(output_folder)
        .await
        .map_err(PlayoutGeneratorError::IoFailure)?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Some(file_name_os) = entry.path().file_stem() {
            let file_name = file_name_os.to_string_lossy();

            if let Some((_, finish)) = parse_playout_filename(&file_name)
                && finish < before
                && let Err(e) = tokio::fs::remove_file(entry.path()).await
            {
                log::warn!(
                    "Failed to remove old playout file {:?}: {}",
                    entry.path(),
                    e
                );
            }
        }
    }

    Ok(())
}
