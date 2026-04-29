use std::path::PathBuf;
use std::time::Duration;

use ersatztv_channel::config::ChannelConfig;
use ersatztv_channel::error::ChannelError;
use tokio::process::Command;

pub struct PtsTime {
    pub duration: Duration,
}

pub struct PtsScanner {
    output_folder: PathBuf,
}

impl PtsScanner {
    pub fn new(channel_config: &ChannelConfig) -> PtsScanner {
        PtsScanner {
            output_folder: channel_config.expanded_output_folder().to_owned(),
        }
    }

    pub async fn get_last_pts(&self) -> Result<PtsTime, ChannelError> {
        let mut pts_time = PtsTime {
            duration: Duration::ZERO,
        };

        // find last segment file in output folder
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&self.output_folder).await?;
        while let Ok(Some(entry)) = dir.next_entry().await {
            if entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
            {
                entries.push(entry);
            }
        }
        entries.sort_by_key(|a| std::cmp::Reverse(a.file_name()));
        if let Some(last_segment) = entries.first() {
            // call ffprobe
            let path = last_segment
                .path()
                .into_os_string()
                .into_string()
                .map_err(|_| ChannelError::PtsScannerFailure)?;

            let output = Command::new("ffprobe")
                .args([
                    "-v",
                    "-0",
                    "-show_entries",
                    "packet=pts_time,duration_time",
                    "-of",
                    "compact=p=0:nk=1",
                    &path,
                ])
                .output()
                .await
                .map_err(|_| ChannelError::PtsScannerFailure)?;

            // parse output line by line for largest pts time
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let split: Vec<&str> = line.trim().split('|').collect();
                if let Ok(seconds) = split[0].parse::<f64>() {
                    let mut total_seconds = seconds;
                    if let Ok(seconds) = split[1].parse::<f64>() {
                        total_seconds += seconds;
                    }

                    let duration = Duration::from_secs_f64(total_seconds);
                    if duration > pts_time.duration {
                        pts_time.duration = duration
                    }
                }
            }
        }

        Ok(pts_time)
    }
}
