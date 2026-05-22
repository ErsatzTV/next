use std::path::PathBuf;

use ersatztv_channel::config::ChannelConfig;
use ersatztv_channel::error::ChannelError;
use ersatztv_playout::playout::PlayoutItem;
use time::OffsetDateTime;

pub struct Dossier {
    channel_config: ChannelConfig,
    item_id: String,
    item_json: String,
    stderr_tail: Vec<String>,
    report_source_file: Option<PathBuf>,
}

impl Dossier {
    pub fn new(
        channel_config: &ChannelConfig,
        item: &PlayoutItem,
        stderr_tail: Vec<String>,
        report_source_file: Option<PathBuf>,
    ) -> Self {
        Self {
            channel_config: channel_config.clone(),
            item_id: item.id.clone(),
            item_json: serde_json::to_string_pretty(item).unwrap_or_else(|_| String::from("{}")),
            stderr_tail,
            report_source_file,
        }
    }

    pub async fn write(&self) -> Result<(), ChannelError> {
        if let Some(reports_folder) = self.channel_config.ffmpeg.reports_folder.as_ref() {
            let timestamp = OffsetDateTime::now_utc().unix_timestamp_nanos();

            let reports_folder = PathBuf::from(reports_folder);
            let dossier_folder = reports_folder.join(format!(
                "{}_{}_{}",
                self.channel_config.number(),
                timestamp,
                self.item_id
            ));

            tokio::fs::create_dir_all(&dossier_folder).await?;

            let mut report_saved = false;
            if let Some(ref report_source_file) = self.report_source_file
                && report_source_file.exists()
            {
                let report_dest_file = dossier_folder.join("ffreport.log");
                if tokio::fs::rename(report_source_file, &report_dest_file)
                    .await
                    .is_ok()
                {
                    report_saved = true;
                }
            }

            if !report_saved {
                let ffmpeg_stderr_file = dossier_folder.join("ffmpeg_stderr.log");
                tokio::fs::write(&ffmpeg_stderr_file, self.stderr_tail.join("\n")).await?;
            }

            let playout_item_file = dossier_folder.join("playout_item.json");
            tokio::fs::write(&playout_item_file, &self.item_json).await?;

            let channel_config_json = serde_json::to_string_pretty(&self.channel_config)
                .unwrap_or_else(|_| String::from("{}"));
            let channel_config_file = dossier_folder.join("channel_config.json");
            tokio::fs::write(&channel_config_file, &channel_config_json).await?;
        }

        Ok(())
    }
}
