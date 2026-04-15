use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ersatztv_core::empty_folder;
use ersatztv_playout::playout::{DATE_FORMAT, Playout, PlayoutItem};
use sqlx::{FromRow, SqlitePool};
use time::OffsetDateTime;

use crate::error::PlayoutGeneratorError;

#[derive(FromRow)]
#[sqlx(rename_all = "PascalCase")]
struct PlayoutChannel {
    id: i32,
    name: String,
}

#[derive(FromRow)]
struct PlayoutItemWithPath {
    #[sqlx(rename = "InPoint")]
    in_point: Option<String>,
    #[sqlx(rename = "OutPoint")]
    out_point: Option<String>,
    #[sqlx(rename = "Start")]
    start: OffsetDateTime,
    #[sqlx(rename = "Finish")]
    finish: OffsetDateTime,
    #[sqlx(rename = "Path")]
    path: String,
}

pub async fn sync_playout(
    database: &PathBuf,
    channel_number: &String,
    output_folder: &Path,
) -> Result<(), PlayoutGeneratorError> {
    let pool = SqlitePool::connect(&format!("sqlite:{}", database.to_string_lossy())).await?;

    let playout =
        sqlx::query_as::<_, PlayoutChannel>("SELECT P.Id, C.Name FROM Channel C INNER JOIN Playout P ON P.ChannelId = C.Id WHERE C.Number = ?")
            .bind(channel_number)
            .fetch_one(&pool)
            .await?;

    log::info!(
        "synchronizing channel {} with playout {} from database at {:?}",
        playout.name,
        playout.id,
        database
    );

    let items_with_paths = sqlx::query_as::<_, PlayoutItemWithPath>(
        "SELECT
        PI.InPoint, PI.OutPoint, PI.Start, PI.Finish, MF.Path
    FROM PlayoutItem PI
    INNER JOIN MediaVersion MV ON
        PI.MediaItemId = MV.MovieId OR
        PI.MediaItemId = MV.EpisodeId OR
        PI.MediaItemId = MV.MusicVideoId OR
        PI.MediaItemId = MV.OtherVideoId
    INNER JOIN MediaFile MF ON MV.Id = MF.MediaVersionId
    INNER JOIN LibraryFolder LF on MF.LibraryFolderId = LF.Id
    INNER JOIN LibraryPath LP on LF.LibraryPathId = LP.Id
    INNER JOIN LocalLibrary L on LP.LibraryId = L.Id
    WHERE PI.PlayoutId = ?",
    )
    .bind(playout.id)
    .fetch_all(&pool)
    .await?;

    log::debug!("found {} playout items", items_with_paths.len());

    let mut items: Vec<PlayoutItem> = Vec::new();
    for item in items_with_paths {
        if let Ok(playout_item) = PlayoutItem::new(
            uuid::Uuid::new_v4().to_string(),
            item.start,
            item.finish,
            item.in_point.as_deref().and_then(parse_duration),
            item.out_point.as_deref().and_then(parse_duration),
            Path::new(&item.path),
        ) {
            items.push(playout_item);
        }
    }

    log::debug!(
        "{} of the playout items were from local libraries",
        items.len()
    );

    empty_folder(output_folder).await?;

    let mut groups: BTreeMap<time::Date, Vec<PlayoutItem>> = BTreeMap::new();
    for item in items {
        groups.entry(item.start.date()).or_default().push(item);
    }

    for (_, group_items) in groups {
        let file_start = group_items.first().unwrap().start;
        let file_finish = group_items.last().unwrap().finish;
        let formatted_start = file_start.format(&DATE_FORMAT)?;
        let formatted_finish = file_finish.format(&DATE_FORMAT)?;
        let output_file = format!("{formatted_start}_{formatted_finish}.json");
        let output_path = output_folder.join(&output_file);
        let playout = Playout::new(group_items);
        let output_string = serde_json::to_string_pretty(&playout)?;
        tokio::fs::write(&output_path, output_string).await?;
    }

    Ok(())
}

fn parse_duration(s: &str) -> Option<Duration> {
    // Expected format: "00:22:41.9850000" (HH:MM:SS.sssssss)
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: u64 = parts[0].parse().ok()?;
    let minutes: u64 = parts[1].parse().ok()?;

    // Split seconds and subseconds
    let sec_parts: Vec<&str> = parts[2].split('.').collect();
    let seconds: u64 = sec_parts[0].parse().ok()?;

    let mut duration =
        Duration::from_hours(hours) + Duration::from_mins(minutes) + Duration::from_secs(seconds);

    // Handle fractional seconds (subseconds)
    if sec_parts.len() == 2 {
        let frac_str = sec_parts[1];
        // We only need the first 9 digits for nanoseconds
        let nanosecs: u64 = format!("{:0<9}", &frac_str[..std::cmp::min(9, frac_str.len())])
            .parse()
            .ok()?;
        duration += Duration::from_nanos(nanosecs);
    }

    Some(duration)
}
