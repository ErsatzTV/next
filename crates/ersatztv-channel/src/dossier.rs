use std::path::PathBuf;

use ersatztv_channel::config::ChannelConfig;
use ersatztv_channel::error::ChannelError;
use ersatztv_playout::playout::{DATE_FORMAT, PlayoutItem};
use ffpipeline::ffmpeg_info::FfmpegInfo;
use ffpipeline::hw_accel::HardwareAccel;
use ffpipeline::probe::ProbeResult;
use serde::Serialize;
use time::OffsetDateTime;

#[derive(Default, Serialize)]
struct MediaInfo {
    video: serde_json::Value,
    audio: serde_json::Value,
    subtitle: serde_json::Value,
}

#[derive(Serialize)]
struct Pipeline {
    ffmpeg_info: FfmpegInfo,
    hw_accel: Option<HardwareAccel>,
}

pub struct Dossier {
    channel_config: ChannelConfig,
    pipeline: Pipeline,
    item_id: Option<String>,
    item_json: Option<String>,
    media_info: Option<MediaInfo>,
    stderr_tail: Option<Vec<String>>,
    report_source_file: Option<PathBuf>,
}

impl Dossier {
    pub async fn write(&self) -> Result<(), ChannelError> {
        if let Some(reports_folder) = self.channel_config.ffmpeg.reports_folder.as_ref() {
            let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
            let formatted_now = now.format(&DATE_FORMAT)?;

            let reports_folder = PathBuf::from(reports_folder);
            let dossier_folder = if let Some(item_id) = &self.item_id {
                reports_folder.join(format!(
                    "{}_{}_{}",
                    self.channel_config.number(),
                    formatted_now,
                    item_id
                ))
            } else {
                reports_folder.join(format!(
                    "{}_{}",
                    self.channel_config.number(),
                    formatted_now
                ))
            };

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

            if !report_saved
                && let Some(stderr_tail) = self.stderr_tail.as_ref().filter(|t| !t.is_empty())
            {
                let ffmpeg_stderr_file = dossier_folder.join("ffmpeg_stderr.log");
                tokio::fs::write(&ffmpeg_stderr_file, stderr_tail.join("\n")).await?;
            }

            let pipeline_json =
                serde_json::to_string_pretty(&self.pipeline).unwrap_or_else(|_| String::from("{}"));
            let pipeline_file = dossier_folder.join("pipeline.json");
            tokio::fs::write(&pipeline_file, pipeline_json).await?;

            if let Some(item_json) = &self.item_json {
                let playout_item_file = dossier_folder.join("playout_item.json");
                tokio::fs::write(&playout_item_file, item_json).await?;
            }

            if let Some(media_info) = &self.media_info {
                let media_info_json =
                    serde_json::to_string_pretty(media_info).unwrap_or_else(|_| String::from("{}"));
                let media_info_file = dossier_folder.join("media_info.json");
                tokio::fs::write(&media_info_file, media_info_json).await?;
            }

            let channel_config_json = serde_json::to_string_pretty(&self.channel_config)
                .unwrap_or_else(|_| String::from("{}"));
            let channel_config_file = dossier_folder.join("channel_config.json");
            tokio::fs::write(&channel_config_file, &channel_config_json).await?;
        }

        Ok(())
    }
}

pub struct DossierBuilder {
    channel_config: ChannelConfig,
    pipeline: Pipeline,
    item_id: Option<String>,
    item_json: Option<String>,
    media_info: Option<MediaInfo>,
    stderr_tail: Option<Vec<String>>,
    report_source_file: Option<PathBuf>,
}

impl DossierBuilder {
    pub fn new(channel_config: &ChannelConfig, ffmpeg_info: &FfmpegInfo) -> DossierBuilder {
        DossierBuilder {
            channel_config: channel_config.clone(),
            pipeline: Pipeline {
                ffmpeg_info: ffmpeg_info.clone(),
                hw_accel: None,
            },
            item_id: None,
            item_json: None,
            media_info: None,
            stderr_tail: None,
            report_source_file: None,
        }
    }

    pub fn item(mut self, item: &PlayoutItem) -> DossierBuilder {
        self.item_id = Some(item.id.clone());
        self.item_json =
            Some(serde_json::to_string_pretty(item).unwrap_or_else(|_| String::from("{}")));
        self
    }

    pub fn stderr(mut self, stderr_tail: Vec<String>) -> DossierBuilder {
        self.stderr_tail = Some(stderr_tail);
        self
    }

    pub fn report_source(mut self, report_source_file: PathBuf) -> DossierBuilder {
        self.report_source_file = Some(report_source_file);
        self
    }

    pub fn video(mut self, video_probe_result: &ProbeResult) -> DossierBuilder {
        let value = serde_json::to_value(video_probe_result).unwrap_or(serde_json::Value::Null);
        self.media_info.get_or_insert_with(MediaInfo::default).video = value;
        self
    }

    pub fn audio(mut self, audio_probe_result: &ProbeResult) -> DossierBuilder {
        let value = serde_json::to_value(audio_probe_result).unwrap_or(serde_json::Value::Null);
        self.media_info.get_or_insert_with(MediaInfo::default).audio = value;
        self
    }

    pub fn subtitle(mut self, subtitle_probe_result: &ProbeResult) -> DossierBuilder {
        let value = serde_json::to_value(subtitle_probe_result).unwrap_or(serde_json::Value::Null);
        self.media_info
            .get_or_insert_with(MediaInfo::default)
            .subtitle = value;
        self
    }

    pub fn accel(mut self, accel: &HardwareAccel) -> DossierBuilder {
        self.pipeline.hw_accel = Some(accel.clone());
        self
    }

    pub fn build(self) -> Dossier {
        Dossier {
            channel_config: self.channel_config,
            pipeline: self.pipeline,
            item_id: self.item_id,
            item_json: self.item_json,
            media_info: self.media_info,
            stderr_tail: self.stderr_tail,
            report_source_file: self.report_source_file,
        }
    }
}
