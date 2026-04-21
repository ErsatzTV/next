use std::collections::HashMap;
use std::path::PathBuf;

use ersatztv_playout::playout::{
    DATE_FORMAT, Playout, PlayoutItem, PlayoutItemSource, PlayoutItemTracks, TrackSelection,
};
use ffpipeline::probe::{ProbeResult, ProbeResultStream};
use rand::RngExt;
use rand::prelude::SliceRandom;
use time::OffsetDateTime;
use walkdir::WalkDir;

use crate::error::PlayoutGeneratorError;
use crate::{IMAGE_EXTENSIONS, is_image_extension, is_video_extension, to_probe_result};

pub async fn generate_playout(
    content_folder: &PathBuf,
    output_folder: &PathBuf,
) -> Result<(), PlayoutGeneratorError> {
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

    if video_paths.is_empty() {
        return Err(PlayoutGeneratorError::NoSourceContent);
    }

    // generate output file name
    let start = OffsetDateTime::now_local()?.truncate_to_day();
    let formatted_start = start.format(&DATE_FORMAT)?;
    let finish = start + time::Duration::days(1) - time::Duration::seconds(1);

    let video_list: Vec<(PathBuf, ProbeResult)> = video_paths.into_iter().collect();

    // fill output file with content
    let mut playout_items: Vec<PlayoutItem> = Vec::new();
    let mut current_time = start;
    let mut rng = rand::rng();
    let mut shuffled = video_list.clone();
    shuffled.shuffle(&mut rng);
    let mut cursor = 0;

    while current_time < finish {
        if cursor >= shuffled.len() {
            let last = shuffled.last().cloned();
            shuffled.shuffle(&mut rng);
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
                });

                playout_item.source = None;
            }

            playout_items.push(playout_item);
            current_time += scheduled_duration;
        }
    }

    let formatted_finish = current_time.format(&DATE_FORMAT)?;
    let output_file = format!("{formatted_start}_{formatted_finish}.json");
    log::debug!("output_file: {output_file}");

    if !output_folder.exists() {
        tokio::fs::create_dir_all(output_folder).await?;
    }

    let output_path = output_folder.join(&output_file);
    let playout = Playout::new(playout_items);
    let output_string = serde_json::to_string_pretty(&playout)?;
    tokio::fs::write(&output_path, output_string).await?;

    Ok(())
}
