use ersatztv_playout::playout::{DATE_FORMAT, PlayoutItem, PlayoutLoadResult};
use time::OffsetDateTime;

use crate::config::ChannelConfig;
use crate::error::ChannelError;

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

                if path.ends_with(".json") {
                    let split: Vec<&str> = file_name.split("_").collect();
                    if split.len() == 2 {
                        let maybe_start = OffsetDateTime::parse(split[0], &DATE_FORMAT)
                            .ok()
                            .or_else(|| parse_unix_timestamp(split[0]));

                        let maybe_finish = OffsetDateTime::parse(split[1], &DATE_FORMAT)
                            .ok()
                            .or_else(|| parse_unix_timestamp(split[1]));

                        match (maybe_start, maybe_finish) {
                            (Some(start), Some(finish)) => {
                                log::trace!(
                                    "Successfully parsed playout date range ({}, {}).",
                                    start,
                                    finish
                                );
                                if now >= &start && now < &finish {
                                    return Ok(path);
                                }
                            }
                            _ => {
                                log::debug!(
                                    "Could not parse start and end time for filename {file_name}"
                                )
                            }
                        }
                    }
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

fn parse_unix_timestamp(timestamp: &str) -> Option<OffsetDateTime> {
    let maybe_epoch = timestamp
        .parse::<i64>()
        .map(|i| if timestamp.len() > 10 { i / 1000 } else { i });

    if let Ok(epoch) = maybe_epoch {
        // TODO: Could use more logging maybe
        OffsetDateTime::from_unix_timestamp(epoch).ok()
    } else {
        log::trace!("Couldn't parse Unix timestamp ({})", timestamp);
        None
    }
}
