use ersatztv_channel::config::ChannelConfig;
use ersatztv_channel::error::ChannelError;
use ersatztv_playout::playout::{PlayoutItem, PlayoutLoadResult, parse_playout_filename};
use time::OffsetDateTime;

pub struct PlayoutLoader {
    channel_config: ChannelConfig,
}

impl PlayoutLoader {
    pub fn new(channel_config: &ChannelConfig) -> PlayoutLoader {
        PlayoutLoader {
            channel_config: channel_config.to_owned(),
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

        // in case current item isn't found
        let next_start = self.next_start(&playout_result, now);

        // find current item
        playout_result
            .playout
            .items
            .into_iter()
            .rfind(|i| now >= &i.start && now < &i.finish())
            .ok_or(ChannelError::PlayoutJsonNoItem { next_start })
    }

    /// A gap before a later item is true, so troubleshooting reaches the gap and fails.
    pub async fn has_remaining(&self, now: &OffsetDateTime) -> Result<bool, ChannelError> {
        match self.get_current_item(now).await {
            Ok(_)
            | Err(ChannelError::PlayoutJsonNoItem {
                next_start: Some(_),
            }) => Ok(true),
            Err(ChannelError::PlayoutJsonNoFileForTime(_))
            | Err(ChannelError::PlayoutJsonNoItem { next_start: None }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn playout_file_for_time(&self, now: &OffsetDateTime) -> Result<String, ChannelError> {
        let mut entries = tokio::fs::read_dir(self.channel_config.expanded_playout_folder())
            .await
            .map_err(|e| {
                ChannelError::ChannelConfigFailure(format!(
                    "{}: {:?}",
                    e,
                    self.channel_config.expanded_playout_folder()
                ))
            })?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path =
                entry.path().into_os_string().into_string().map_err(|_| {
                    ChannelError::ChannelConfigFailure(String::from("os string error"))
                })?;

            if let Some(file_name_os) = entry.path().file_stem() {
                let file_name = file_name_os.to_os_string().into_string().map_err(|_| {
                    ChannelError::ChannelConfigFailure(String::from("os string error"))
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
            .playout
            .items
            .iter()
            .find(|i| &i.start > now)
            .map(|i| i.start)
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
        let file_name = format!(
            "{}_{}.json",
            start.unix_timestamp() * 1000,
            finish.unix_timestamp() * 1000
        );
        tokio::fs::write(
            folder.join(file_name),
            serde_json::to_vec(&Playout::new(items)).unwrap(),
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
}
