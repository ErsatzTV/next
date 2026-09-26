use ersatztv_channel::config::ChannelConfig;
use ersatztv_channel::error::{ChannelError, IoContext};
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

    async fn playout_file_for_time(&self, now: &OffsetDateTime) -> Result<String, ChannelError> {
        let mut entries = tokio::fs::read_dir(self.channel_config.expanded_playout_folder())
            .await
            .io_context(
                "scan the playout folder",
                self.channel_config.expanded_playout_folder(),
            )?;
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
            .playout
            .items
            .iter()
            .find(|i| &i.start > now)
            .map(|i| i.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANNEL: &str = r#"{
        "playout": { "folder": "no-such-playout-folder" },
        "ffmpeg": { "ffmpeg_path": "/usr/bin/ffmpeg" },
        "normalization": {
            "audio": { "format": "aac", "bitrate_kbps": 192 },
            "video": { "format": "h264", "bit_depth": 8, "bitrate_kbps": 4000 }
        }
    }"#;

    /// A missing playout folder is a storage fault, not a configuration one.
    /// Reported as a config failure it sends the operator to channel.json,
    /// which is the wrong file and the expensive half of the diagnosis.
    #[tokio::test]
    async fn a_missing_playout_folder_is_not_reported_as_a_config_failure() {
        let folder = tempfile::tempdir().unwrap();
        let config_path = folder.path().join("channel.json");
        tokio::fs::write(&config_path, CHANNEL).await.unwrap();

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
            message.contains("scan the playout folder"),
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
}
