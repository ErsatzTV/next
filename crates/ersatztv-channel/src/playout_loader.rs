use std::sync::Mutex;
use std::time::SystemTime;

use ersatztv_channel::config::ChannelConfig;
use ersatztv_channel::error::{ChannelError, IoContext};
use ersatztv_playout::playout::{
    ItemIssueKind, LoadedItem, PlayoutItem, PlayoutLoadResult, parse_playout_filename,
};
use time::OffsetDateTime;

pub struct PlayoutLoader {
    channel_config: ChannelConfig,
    // the file is re-read on every selection; log issues loudly once per file version
    reported_file: Mutex<Option<(String, Option<SystemTime>)>>,
}

impl PlayoutLoader {
    pub fn new(channel_config: &ChannelConfig) -> PlayoutLoader {
        PlayoutLoader {
            channel_config: channel_config.to_owned(),
            reported_file: Mutex::new(None),
        }
    }

    pub async fn get_current_item(
        &self,
        now: &OffsetDateTime,
    ) -> Result<PlayoutItem, ChannelError> {
        // TODO: refactor selecting playout file

        log::debug!(
            "playout folder is {}",
            self.channel_config
                .expanded_playout_folder()
                .to_string_lossy()
        );

        let path = self.playout_file_for_time(now).await?;
        log::debug!("playout JSON is {path}");

        // load playout JSON
        let playout_result = ersatztv_playout::playout::from_file(&path).await?;
        self.report_issues(&path, &playout_result).await;

        // in case current item isn't found
        let next_start = self.next_start(&playout_result, now);

        // find current item
        match playout_result
            .items
            .into_iter()
            .rfind(|i| now >= &i.start() && now < &i.finish())
        {
            Some(LoadedItem::Valid(item)) => Ok(*item),
            Some(LoadedItem::Invalid(item)) => Err(ChannelError::PlayoutItemInvalid {
                item_id: item.id,
                start: item.start,
                finish: item.finish,
                error: item.error,
            }),
            None => Err(ChannelError::PlayoutJsonNoItem { next_start }),
        }
    }

    async fn report_issues(&self, path: &str, playout_result: &PlayoutLoadResult) {
        if playout_result.issues.is_empty() {
            return;
        }

        let modified = tokio::fs::metadata(path)
            .await
            .and_then(|m| m.modified())
            .ok();
        let key = (path.to_owned(), modified);
        let first_report = {
            let mut reported = self.reported_file.lock().unwrap_or_else(|e| e.into_inner());
            let first = reported.as_ref() != Some(&key);
            *reported = Some(key);
            first
        };

        for issue in &playout_result.issues {
            let item = match &issue.id {
                Some(id) => format!("item {id} (#{})", issue.index),
                None => format!("item #{}", issue.index),
            };
            let (level, message) = match issue.kind {
                ItemIssueKind::ProbeHintDropped => (
                    log::Level::Warn,
                    format!("{item} has an invalid probe hint, playing without hints"),
                ),
                ItemIssueKind::Invalid => (
                    log::Level::Error,
                    format!("{item} is invalid and will be replaced with fallback"),
                ),
                ItemIssueKind::Dropped => (
                    log::Level::Error,
                    format!("{item} has no readable id, start and finish and is skipped"),
                ),
            };
            let level = if first_report {
                level
            } else {
                log::Level::Debug
            };
            log::log!(level, "playout JSON {path}: {message}: {}", issue.error);
        }
    }

    /// A gap before a later item is true, so troubleshooting reaches the gap and fails.
    pub async fn has_remaining(&self, now: &OffsetDateTime) -> Result<bool, ChannelError> {
        match self.get_current_item(now).await {
            Ok(_)
            | Err(ChannelError::PlayoutItemInvalid { .. })
            | Err(ChannelError::PlayoutJsonNoItem {
                next_start: Some(_),
            }) => Ok(true),
            Err(ChannelError::PlayoutJsonNoFileForTime(_))
            | Err(ChannelError::PlayoutJsonNoItem { next_start: None }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn playout_file_for_time(&self, now: &OffsetDateTime) -> Result<String, ChannelError> {
        // resolve the link once; it can be swapped before the file is read
        let folder = tokio::fs::canonicalize(self.channel_config.expanded_playout_folder())
            .await
            .io_context(
                "resolve the playout folder",
                self.channel_config.expanded_playout_folder(),
            )?;
        let mut entries = tokio::fs::read_dir(&folder)
            .await
            .io_context("scan the playout folder", &folder)?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path =
                entry.path().into_os_string().into_string().map_err(|p| {
                    ChannelError::PlayoutPathNotUtf8(p.to_string_lossy().into_owned())
                })?;

            if let Some(file_name_os) = entry.path().file_stem() {
                let file_name = file_name_os.to_os_string().into_string().map_err(|p| {
                    ChannelError::PlayoutPathNotUtf8(p.to_string_lossy().into_owned())
                })?;

                if let Some((start, finish)) = parse_playout_filename(file_name.as_str())
                    && now >= &start
                    && now < &finish
                {
                    return Ok(path);
                }
            }
        }

        Err(ChannelError::PlayoutJsonNoFileForTime(*now))
    }

    fn next_start(
        &self,
        playout_result: &PlayoutLoadResult,
        now: &OffsetDateTime,
    ) -> Option<OffsetDateTime> {
        playout_result
            .items
            .iter()
            .map(LoadedItem::start)
            .find(|start| start > now)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ersatztv_playout::playout::Playout;
    use serde_json::json;
    use time::Duration;

    use super::*;

    async fn loader(folder: &Path, items: Vec<PlayoutItem>) -> PlayoutLoader {
        let start = items.first().unwrap().start;
        let finish = items.last().unwrap().finish();
        let playout = serde_json::to_value(Playout::new(items)).unwrap();
        json_loader(folder, start, finish, playout).await
    }

    async fn json_loader(
        folder: &Path,
        start: OffsetDateTime,
        finish: OffsetDateTime,
        playout: serde_json::Value,
    ) -> PlayoutLoader {
        let file_name = format!(
            "{}_{}.json",
            start.unix_timestamp() * 1000,
            finish.unix_timestamp() * 1000
        );
        tokio::fs::write(
            folder.join(file_name),
            serde_json::to_vec(&playout).unwrap(),
        )
        .await
        .unwrap();

        let config_dir = tempfile::tempdir().unwrap();
        let config_path = config_dir.path().join("channel.json");
        let config = json!({
            "version": ersatztv_channel::config::SCHEMA.uri(),
            "playout": { "folder": folder },
            "ffmpeg": {},
            "normalization": {
                "audio": { "format": "aac" },
                "video": { "format": "h264", "bit_depth": 8 }
            }
        });
        tokio::fs::write(&config_path, serde_json::to_vec(&config).unwrap())
            .await
            .unwrap();
        let channel_config =
            ChannelConfig::from_sources(&[config_path], &config_dir.path().to_path_buf(), "1")
                .await
                .unwrap();

        PlayoutLoader::new(&channel_config)
    }

    fn item(id: &str, start: OffsetDateTime, seconds: i64) -> PlayoutItem {
        PlayoutItem::new(
            id.to_owned(),
            start,
            start + Duration::seconds(seconds),
            None,
            None,
            Path::new("/media/item.mkv"),
        )
        .unwrap()
    }

    // swap the link the same way legacy does
    fn link_version(link: &Path, version: &Path) {
        #[cfg(unix)]
        {
            let temp_link = link.with_extension("tmp");
            std::os::unix::fs::symlink(version.file_name().unwrap(), &temp_link).unwrap();
            std::fs::rename(&temp_link, link).unwrap();
        }

        #[cfg(windows)]
        {
            if link.exists() {
                std::fs::remove_dir(link).unwrap();
            }

            let status = std::process::Command::new("cmd.exe")
                .args(["/c", "mklink", "/j"])
                .arg(link)
                .arg(version)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
        }
    }

    #[tokio::test]
    async fn listed_playout_file_survives_folder_swap() {
        let root = tempfile::tempdir().unwrap();
        let old_version = root.path().join("1000");
        let new_version = root.path().join("2000");
        let current = root.path().join("current");
        tokio::fs::create_dir(&old_version).await.unwrap();
        tokio::fs::create_dir(&new_version).await.unwrap();
        link_version(&current, &old_version);

        let t0 = OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap();
        let loader = loader(&current, vec![item("a", t0, 10)]).await;
        let path = loader.playout_file_for_time(&t0).await.unwrap();

        link_version(&current, &new_version);

        assert!(ersatztv_playout::playout::from_file(&path).await.is_ok());
    }

    #[tokio::test]
    async fn has_remaining_until_the_playout_ends() {
        let folder = tempfile::tempdir().unwrap();
        let t0 = OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap();
        let loader = loader(
            folder.path(),
            vec![item("a", t0, 10), item("b", t0 + Duration::seconds(20), 10)],
        )
        .await;

        assert!(loader.has_remaining(&t0).await.unwrap());
        assert!(
            loader
                .has_remaining(&(t0 + Duration::seconds(15)))
                .await
                .unwrap()
        );
        assert!(
            !loader
                .has_remaining(&(t0 + Duration::seconds(30)))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn a_missing_playout_folder_is_not_reported_as_a_config_failure() {
        let folder = tempfile::tempdir().unwrap();
        let config_path = folder.path().join("channel.json");
        let config = json!({
            "version": ersatztv_channel::config::SCHEMA.uri(),
            "playout": { "folder": "no-such-playout-folder" },
            "ffmpeg": {},
            "normalization": {
                "audio": { "format": "aac" },
                "video": { "format": "h264", "bit_depth": 8 }
            }
        });
        tokio::fs::write(&config_path, serde_json::to_vec(&config).unwrap())
            .await
            .unwrap();

        let channel_config =
            ChannelConfig::from_sources(&[config_path], &folder.path().join("out"), "5")
                .await
                .unwrap();
        let expected = channel_config
            .expanded_playout_folder()
            .display()
            .to_string();
        let loader = PlayoutLoader::new(&channel_config);

        let message = loader
            .get_current_item(&OffsetDateTime::UNIX_EPOCH)
            .await
            .unwrap_err()
            .to_string();

        assert!(
            message.contains("resolve the playout folder"),
            "message does not name the operation: {message}"
        );
        assert!(
            message.contains(&expected),
            "message does not name the folder: {message}"
        );
        assert!(
            !message.contains("channel config"),
            "message blames the channel config: {message}"
        );
    }

    fn item_json(id: &str, start: OffsetDateTime, seconds: i64) -> serde_json::Value {
        serde_json::to_value(item(id, start, seconds)).unwrap()
    }

    async fn items_loader(
        folder: &Path,
        start: OffsetDateTime,
        finish: OffsetDateTime,
        items: Vec<serde_json::Value>,
    ) -> PlayoutLoader {
        let playout = json!({
            "version": ersatztv_playout::playout::SCHEMA.uri(),
            "items": items
        });
        json_loader(folder, start, finish, playout).await
    }

    #[tokio::test]
    async fn invalid_item_fails_only_its_own_slot() {
        let folder = tempfile::tempdir().unwrap();
        let t0 = OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap();
        let mut bad = item_json("bad", t0 + Duration::seconds(10), 10);
        bad["source"] = json!({ "source_type": "carrier_pigeon" });
        let loader = items_loader(
            folder.path(),
            t0,
            t0 + Duration::seconds(30),
            vec![
                item_json("a", t0, 10),
                bad,
                item_json("c", t0 + Duration::seconds(20), 10),
            ],
        )
        .await;

        let at = |seconds| t0 + Duration::seconds(seconds);
        assert_eq!(loader.get_current_item(&at(5)).await.unwrap().id, "a");
        match loader.get_current_item(&at(15)).await {
            Err(ChannelError::PlayoutItemInvalid {
                item_id,
                start,
                finish,
                error,
            }) => {
                assert_eq!(item_id, "bad");
                assert_eq!((start, finish), (at(10), at(20)));
                assert!(error.contains("carrier_pigeon"), "{error}");
            }
            other => panic!("expected PlayoutItemInvalid, got {other:?}"),
        }
        assert!(loader.has_remaining(&at(15)).await.unwrap());
        assert_eq!(loader.get_current_item(&at(25)).await.unwrap().id, "c");
    }

    #[tokio::test]
    async fn gap_before_an_invalid_item_ends_at_its_start() {
        let folder = tempfile::tempdir().unwrap();
        let t0 = OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap();
        let mut bad = item_json("bad", t0 + Duration::seconds(20), 10);
        bad["source"] = json!({ "source_type": "carrier_pigeon" });
        let loader = items_loader(
            folder.path(),
            t0,
            t0 + Duration::seconds(30),
            vec![item_json("a", t0, 10), bad],
        )
        .await;

        assert!(matches!(
            loader.get_current_item(&(t0 + Duration::seconds(15))).await,
            Err(ChannelError::PlayoutJsonNoItem { next_start: Some(s) }) if s == t0 + Duration::seconds(20)
        ));
    }

    #[tokio::test]
    async fn dropped_item_is_a_gap_until_the_next_item() {
        let folder = tempfile::tempdir().unwrap();
        let t0 = OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap();
        let mut bad = item_json("bad", t0 + Duration::seconds(10), 10);
        bad["start"] = json!("yesterday");
        let loader = items_loader(
            folder.path(),
            t0,
            t0 + Duration::seconds(30),
            vec![
                item_json("a", t0, 10),
                bad,
                item_json("c", t0 + Duration::seconds(20), 10),
            ],
        )
        .await;

        assert!(matches!(
            loader.get_current_item(&(t0 + Duration::seconds(15))).await,
            Err(ChannelError::PlayoutJsonNoItem { next_start: Some(s) }) if s == t0 + Duration::seconds(20)
        ));
    }

    #[tokio::test]
    async fn item_with_a_bad_probe_hint_plays_without_hints() {
        let folder = tempfile::tempdir().unwrap();
        let t0 = OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap();
        let mut hinted = item_json("a", t0, 10);
        hinted["source"]["probe_hint"] = json!({
            "video": [{ "width": 1920, "height": 1080, "pix_fmt": "yuv420p", "stream_index": 0 }]
        });
        let loader =
            items_loader(folder.path(), t0, t0 + Duration::seconds(10), vec![hinted]).await;

        let item = loader.get_current_item(&t0).await.unwrap();
        assert_eq!(item.id, "a");
        assert!(item.source.unwrap().probe_hint().is_none());
    }
}
