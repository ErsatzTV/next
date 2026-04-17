use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ersatztv_core::{READY_FILE_NAME, empty_folder};
use ersatztv_playout::playout::{
    PlayoutItem, PlayoutItemSource, PlayoutItemTracks, TrackSelection,
};
use ffpipeline::ffmpeg_info::FfmpegInfo;
use ffpipeline::frame_rate::FrameRate;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::input::{InputSettings, InputSource, ProbedInput};
use ffpipeline::output_settings::OutputSettings;
use ffpipeline::pipeline::{AudioFormat, Kbps, PtsOffset, SEGMENT_SECONDS, VideoFormat};
use ffpipeline::probe::ProbeResult;
use ffpipeline::{pipeline, probe};
use simple_expand_tilde::expand_tilde;
use time::OffsetDateTime;
use tokio::sync::Mutex;

use crate::config::ChannelConfig;
use crate::error::ChannelError;
use crate::playlist_manager::PlaylistManager;
use crate::playout_loader::PlayoutLoader;
use crate::pts_scanner::{PtsScanner, PtsTime};

#[derive(Copy, Clone, PartialEq)]
enum ChannelSessionState {
    SeekAndWorkAhead,
    ZeroAndWorkAhead,
    SeekAndRealtime,
    ZeroAndRealtime,
}

impl std::fmt::Display for ChannelSessionState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelSessionState::SeekAndWorkAhead => write!(f, "SeekAndWorkAhead"),
            ChannelSessionState::ZeroAndWorkAhead => write!(f, "ZeroAndWorkAhead"),
            ChannelSessionState::SeekAndRealtime => write!(f, "SeekAndRealtime"),
            ChannelSessionState::ZeroAndRealtime => write!(f, "ZeroAndRealtime"),
        }
    }
}

struct TimingResult {
    in_point: Duration,
    out_point: Duration,
    finish: OffsetDateTime,
    is_complete: bool,
}

pub struct ChannelSession {
    channel_config: ChannelConfig,
    playout_loader: PlayoutLoader,
    pts_scanner: PtsScanner,
    playlist_manager: Arc<Mutex<PlaylistManager>>,

    ffmpeg_path: PathBuf,
    ffprobe_path: PathBuf,
    ffmpeg_info: FfmpegInfo,
    hw_accel: Option<ffpipeline::hw_accel::HardwareAccel>,

    transcoded_until: OffsetDateTime,
    ready_file: PathBuf,

    output_file: String,
    output_segment_template: String,

    start_time_offset: time::Duration,
    state: ChannelSessionState,

    timeout_notify: Arc<tokio::sync::Notify>,
}

impl ChannelSession {
    pub fn new(channel_config: ChannelConfig) -> Result<ChannelSession, ChannelError> {
        let now = OffsetDateTime::now_local()?;

        let start_time_offset = if let Some(virtual_start) = channel_config.playout.virtual_start {
            virtual_start - now
        } else {
            time::Duration::ZERO
        };

        let output_folder = channel_config.expanded_output_folder().to_owned();
        let generated_output_file = output_folder
            .join("live.m3u8")
            .into_os_string()
            .into_string()
            .map_err(|_| ChannelError::ChannelConfigOutputFolderRequired)?;

        let ffmpeg_output_file = output_folder
            .join("ffmpeg.m3u8")
            .into_os_string()
            .into_string()
            .map_err(|_| ChannelError::ChannelConfigOutputFolderRequired)?;

        let output_segment_template = output_folder
            .join("live%06d.ts")
            .into_os_string()
            .into_string()
            .map_err(|_| ChannelError::ChannelConfigOutputFolderRequired)?;

        let ready_file = output_folder.join(READY_FILE_NAME);

        let playout_loader = PlayoutLoader::new(&channel_config);
        let pts_scanner = PtsScanner::new(&channel_config);
        let playlist_manager = PlaylistManager::new(
            now,
            SEGMENT_SECONDS,
            output_folder.to_owned(),
            ready_file.to_owned(),
            generated_output_file,
            ffmpeg_output_file.to_owned(),
        );

        let playlist_manager = Arc::new(Mutex::new(playlist_manager));

        let default_ffprobe_path = Path::new("ffprobe").to_path_buf();
        let default_ffmpeg_path = Path::new("ffmpeg").to_path_buf();

        let ffprobe_path = channel_config
            .ffmpeg
            .ffprobe_path
            .clone()
            .unwrap_or(default_ffprobe_path);
        let ffmpeg_path = channel_config
            .ffmpeg
            .ffmpeg_path
            .clone()
            .unwrap_or(default_ffmpeg_path);

        Ok(ChannelSession {
            channel_config,
            playout_loader,
            pts_scanner,
            playlist_manager,
            ffmpeg_path: ffmpeg_path.to_owned(),
            ffprobe_path: ffprobe_path.to_owned(),
            ffmpeg_info: FfmpegInfo::default(),
            hw_accel: None,
            transcoded_until: now + start_time_offset,
            ready_file,
            output_file: ffmpeg_output_file,
            output_segment_template,
            start_time_offset,
            state: ChannelSessionState::SeekAndWorkAhead,
            timeout_notify: Arc::new(tokio::sync::Notify::new()),
        })
    }

    pub async fn run(&mut self) -> Result<(), ChannelError> {
        self.prep_output_folder().await?;

        self.ffmpeg_info = FfmpegInfo::load(
            &self.ffmpeg_path,
            &self.channel_config.ffmpeg.disabled_filters,
        )
        .await?;

        log::debug!("ffmpeg info: {:?}", self.ffmpeg_info);

        self.hw_accel = self
            .channel_config
            .normalization
            .video
            .accel
            .as_ref()
            .and_then(|a| a.to_pipeline(&self.channel_config));

        let pm = self.playlist_manager.clone();
        let tn = self.timeout_notify.clone();

        tokio::spawn(async move {
            loop {
                let mut playlist_manager = pm.lock().await;
                let _ = playlist_manager.update().await;
                if *playlist_manager.timeout() {
                    tn.notify_one();
                    break;
                }
                drop(playlist_manager);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });

        // always work ahead initially
        let realtime = false;
        self.transcode(realtime).await?;

        let pm = self.playlist_manager.clone();
        let tn = self.timeout_notify.clone();

        loop {
            if *pm.lock().await.timeout() {
                tn.notify_one();
                return Err(ChannelError::IdleTimeout(
                    self.channel_config.number().to_owned(),
                ));
            }

            let now = OffsetDateTime::now_local()? + self.start_time_offset;
            let transcoded_buffer =
                std::cmp::max(time::Duration::ZERO, self.transcoded_until - now);
            log::debug!(
                "transcoded buffer: {}m {}s",
                transcoded_buffer.whole_minutes(),
                transcoded_buffer.whole_seconds() % 60
            );
            if transcoded_buffer <= time::Duration::minutes(1) {
                // only use realtime when we're at least 30 seconds ahead
                let realtime = transcoded_buffer >= time::Duration::seconds(30);
                self.transcode(realtime).await?;
            } else {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    _ = tn.notified() => {
                        return Err(ChannelError::IdleTimeout(self.channel_config.number().to_owned()));
                    }
                }
            }
        }
    }

    async fn prep_output_folder(&self) -> Result<(), ChannelError> {
        let output_folder = self.channel_config.expanded_output_folder();

        if self.ready_file.exists() {
            tokio::fs::remove_file(&self.ready_file).await?;
        }

        if output_folder.exists() {
            empty_folder(output_folder)
                .await
                .map_err(|_| ChannelError::ChannelConfigOutputFolderRequired)?;
        } else {
            tokio::fs::create_dir(output_folder)
                .await
                .map_err(|_| ChannelError::ChannelConfigOutputFolderRequired)?;
        }

        Ok(())
    }

    async fn transcode(&mut self, realtime: bool) -> Result<(), ChannelError> {
        if !realtime {
            log::debug!("channel session will work ahead");

            let next_state = match self.state {
                ChannelSessionState::SeekAndRealtime => ChannelSessionState::SeekAndWorkAhead,
                ChannelSessionState::ZeroAndRealtime => ChannelSessionState::ZeroAndWorkAhead,
                _ => self.state,
            };

            if next_state != self.state {
                log::debug!(
                    "channel session is accelerating {} => {}",
                    self.state,
                    next_state
                );
                self.state = next_state;
            }
        } else {
            log::debug!("channel session will NOT work ahead");

            // throttle to realtime if needed
            let next_state = match self.state {
                ChannelSessionState::SeekAndWorkAhead => ChannelSessionState::SeekAndRealtime,
                ChannelSessionState::ZeroAndWorkAhead => ChannelSessionState::ZeroAndRealtime,
                _ => self.state,
            };

            if next_state != self.state {
                log::debug!(
                    "channel session is throttling {} => {}",
                    self.state,
                    next_state
                );
                self.state = next_state;
            }
        }

        log::debug!("channel session state: {}", self.state);

        // get last pts offset
        let mut pts_time: Option<PtsTime> = None;
        match self.pts_scanner.get_last_pts().await {
            Ok(scanned_pts_time) => pts_time = Some(scanned_pts_time),
            Err(e) => log::debug!("failed to scan pts time: {e}"),
        }

        // TODO: transcode error message instead of bailing
        let current_item_result = self
            .playout_loader
            .get_current_item(&self.transcoded_until)
            .await;

        let current_item = match current_item_result {
            Ok(playout_item) => playout_item,
            Err(ChannelError::PlayoutJsonNoItem { next_start }) => {
                self.fake_playout_item(next_start)
            }
            Err(err) => {
                log::error!("{}", err);
                self.fake_playout_item(None)
            }
        };

        let current_source = current_item.source.clone();
        let audio_source = match current_source.as_ref() {
            Some(source) => Ok(source.to_owned()),
            None => {
                let audio_track_source = current_item
                    .tracks
                    .as_ref()
                    .and_then(|t| t.audio.as_ref())
                    .and_then(|a| match a {
                        TrackSelection::Source { source, .. } => Some(source.to_owned()),
                        _ => None,
                    });
                match audio_track_source {
                    Some(source) => Ok(source),
                    None => Err(ChannelError::PlayoutJsonAudioSourceRequired),
                }
            }
        }?;
        let video_source = match current_source.as_ref() {
            Some(source) => Ok(source.to_owned()),
            None => {
                let video_track_source = current_item
                    .tracks
                    .as_ref()
                    .and_then(|t| t.video.as_ref())
                    .and_then(|v| match v {
                        TrackSelection::Source { source, .. } => Some(source.clone()),
                        _ => None,
                    });
                match video_track_source {
                    Some(source) => Ok(source),
                    None => Err(ChannelError::PlayoutJsonVideoSourceRequired),
                }
            }
        }?;

        let audio_probe_result =
            Self::probe_source(&self.ffprobe_path, &self.ffmpeg_path, &audio_source).await?;
        let video_probe_result = if video_source == audio_source {
            audio_probe_result.clone()
        } else {
            Self::probe_source(&self.ffprobe_path, &self.ffmpeg_path, &video_source).await?
        };

        let audio_norm = &self.channel_config.normalization.audio;
        let video_norm = &self.channel_config.normalization.video;

        let video_size = match (video_norm.width, video_norm.height) {
            (Some(width), Some(height)) => Some(FrameSize { width, height }),
            _ => None,
        };

        // generate pipeline
        let output_settings = OutputSettings {
            audio_format: audio_norm.format.clone().map(AudioFormat::from),
            audio_bitrate: audio_norm.bitrate_kbps.map(Kbps),
            audio_buffer: audio_norm.buffer_kbps.map(Kbps),
            audio_channels: audio_norm.channels,
            video_format: video_norm.format.clone().map(VideoFormat::from),
            bit_depth: video_norm.bit_depth,
            video_bitrate: video_norm.bitrate_kbps.map(Kbps),
            video_buffer: video_norm.buffer_kbps.map(Kbps),
            video_size,
            accel: self.hw_accel.clone(),
            format: ffpipeline::output_format::OutputFormat::Hls {
                playlist: self.output_file.clone(),
                segment_template: self.output_segment_template.clone(),
            },
            pts_offset: pts_time.map(|p| PtsOffset {
                duration: p.duration,
            }),
            realtime,

            frame_rate: if video_probe_result.is_still_image() {
                Some(FrameRate::default())
            } else {
                None
            },
        };

        let start_at_zero = matches!(
            self.state,
            ChannelSessionState::ZeroAndWorkAhead | ChannelSessionState::ZeroAndRealtime
        );

        let audio_timing = self.input_timing(&current_item, &audio_source, start_at_zero, realtime);
        let video_timing = self.input_timing(&current_item, &video_source, start_at_zero, realtime);

        let video_index = current_item
            .tracks
            .as_ref()
            .and_then(|t| t.video.as_ref())
            .and_then(|v| match v {
                TrackSelection::StreamIndex { stream_index } => Some(*stream_index),
                _ => None,
            });

        let audio_index = current_item
            .tracks
            .as_ref()
            .and_then(|t| t.audio.as_ref())
            .and_then(|a| match a {
                TrackSelection::StreamIndex { stream_index } => Some(*stream_index),
                _ => None,
            });

        let input_settings = InputSettings {
            audio_input: ProbedInput {
                input_source: match audio_source {
                    PlayoutItemSource::Local { path, .. } => InputSource::Local { path },
                    PlayoutItemSource::Lavfi { params } => InputSource::Lavfi { params },
                },
                in_point: audio_timing.in_point,
                out_point: audio_timing.out_point,
                probe_result: audio_probe_result,
                audio_index,
                video_index: None,
            },
            video_input: ProbedInput {
                input_source: match video_source {
                    PlayoutItemSource::Local { path, .. } => InputSource::Local { path },
                    PlayoutItemSource::Lavfi { params } => InputSource::Lavfi { params },
                },
                in_point: if video_probe_result.is_still_image() {
                    Duration::ZERO
                } else {
                    video_timing.in_point
                },
                out_point: video_timing.out_point,
                probe_result: video_probe_result,
                audio_index: None,
                video_index,
            },
        };

        let mut pipeline_result =
            pipeline::generate_pipeline(&self.ffmpeg_info, input_settings, output_settings)?;
        pipeline_result.optimize();
        let args = pipeline_result.args();
        let envs = pipeline_result.envs();
        log::debug!("optimized pipeline: {}", args.join(" "));

        self.playlist_manager
            .lock()
            .await
            .before_new_pipeline()
            .await?;

        // stream current item
        let mut ffmpeg_child = tokio::process::Command::new(&self.ffmpeg_path)
            .args(args)
            .envs(envs)
            .stdout(std::process::Stdio::null())
            .spawn()
            .map_err(|_| ChannelError::StreamFailure(String::from("failed to spawn ffmpeg")))?;

        log::debug!("waiting for ffmpeg to terminate...");

        tokio::select! {
            status = ffmpeg_child.wait() => {
                let status = status.map_err(|e| ChannelError::StreamFailure(e.to_string()))?;
                if !status.success() {
                    return Err(ChannelError::StreamFailure(format!(
                        "ffmpeg exited {status}"
                    )));
                }
            }
            _ = self.timeout_notify.notified() => {
                ffmpeg_child.kill().await.ok();
                return Err(ChannelError::IdleTimeout(self.channel_config.number().to_owned()));
            }
        }

        self.transcoded_until = std::cmp::min(audio_timing.finish, video_timing.finish);
        log::debug!("transcoded until: {}", self.transcoded_until);

        self.state = Self::next_state(
            self.state,
            audio_timing.is_complete && video_timing.is_complete,
        );

        Ok(())
    }

    fn next_state(state: ChannelSessionState, is_complete: bool) -> ChannelSessionState {
        let result = match state {
            // after seeking and NOT completing the item, seek again, transcode will accelerate if needed
            ChannelSessionState::SeekAndWorkAhead if !is_complete => {
                ChannelSessionState::SeekAndRealtime
            }

            // after seeking and completing the item, start at zero
            ChannelSessionState::SeekAndWorkAhead => ChannelSessionState::ZeroAndWorkAhead,

            // after starting at zero and NOT completing the item, seek, transcode will accelerate if needed
            ChannelSessionState::ZeroAndWorkAhead if !is_complete => {
                ChannelSessionState::SeekAndRealtime
            }

            // after starting at zero and completing the item, start at zero again, transcode method will throttle if needed
            ChannelSessionState::ZeroAndWorkAhead => ChannelSessionState::ZeroAndWorkAhead,

            // realtime will always complete items, so start next at zero
            ChannelSessionState::SeekAndRealtime => ChannelSessionState::ZeroAndRealtime,

            // realtime will always complete items, so start next at zero
            ChannelSessionState::ZeroAndRealtime => ChannelSessionState::ZeroAndRealtime,
        };

        log::debug!("channel session state {} => {}", state, result);

        result
    }

    async fn probe_source(
        ffprobe_path: &Path,
        ffmpeg_path: &Path,
        source: &PlayoutItemSource,
    ) -> Result<ProbeResult, ChannelError> {
        match source {
            PlayoutItemSource::Local { path, .. } => {
                let expanded_path_buf =
                    expand_tilde(path).ok_or(ChannelError::PlayoutJsonInvalidLocalSource)?;
                let expanded_path = expanded_path_buf
                    .as_os_str()
                    .to_str()
                    .ok_or(ChannelError::PlayoutJsonInvalidLocalSource)?;

                // probe current item
                let probe_result = probe::probe(ffprobe_path, expanded_path).await?;

                Ok(probe_result)
            }
            PlayoutItemSource::Lavfi { params } => {
                let probe_result = probe::probe_lavfi(ffprobe_path, ffmpeg_path, params).await?;

                Ok(probe_result)
            }
        }
    }

    fn input_timing(
        &self,
        current_item: &PlayoutItem,
        source: &PlayoutItemSource,
        start_at_zero: bool,
        realtime: bool,
    ) -> TimingResult {
        let mut is_complete = true;

        let item_start = current_item.start;
        let item_finish = current_item.finish;
        let item_duration = current_item.finish - current_item.start;
        let item_in_point_base_ms = match source {
            PlayoutItemSource::Local { in_point_ms, .. } => in_point_ms.unwrap_or(0),
            _ => 0,
        };
        let item_out_point_ms = match source {
            PlayoutItemSource::Local { out_point_ms, .. } => out_point_ms
                .unwrap_or(item_in_point_base_ms + item_duration.whole_milliseconds() as u64),
            _ => item_in_point_base_ms + item_duration.whole_milliseconds() as u64,
        };

        let effective_now = if start_at_zero {
            item_start
        } else {
            self.transcoded_until
        };

        let progress_ms = if start_at_zero {
            0
        } else {
            (effective_now - item_start).whole_milliseconds().max(0) as u64
        };
        let effective_in_point = Duration::from_millis(item_in_point_base_ms + progress_ms);

        let duration =
            Duration::from_millis((item_finish - effective_now).whole_milliseconds() as u64);

        let limit = if realtime {
            Duration::ZERO
        } else {
            Duration::from_secs(SEGMENT_SECONDS as u64 * 11u64)
        };

        let mut finish = item_finish;
        let mut out_point = Duration::from_millis(item_out_point_ms);

        if limit > Duration::ZERO && duration > limit {
            finish = effective_now + limit;
            out_point = effective_in_point + limit;
            is_complete = false;
        }

        TimingResult {
            in_point: effective_in_point,
            out_point,
            finish,
            is_complete,
        }
    }

    fn fake_playout_item(&self, next_start: Option<OffsetDateTime>) -> PlayoutItem {
        PlayoutItem {
            id: uuid::Uuid::new_v4().to_string(),
            start: self.transcoded_until,
            finish: next_start.unwrap_or(self.transcoded_until + Duration::from_mins(1)),
            source: None,
            tracks: Some(PlayoutItemTracks {
                audio: Some(TrackSelection::Source {
                    source: PlayoutItemSource::Lavfi {
                        params: String::from("anullsrc=channel_layout=stereo:sample_rate=48000"),
                    },
                }),
                video: Some(TrackSelection::Source {
                    source: PlayoutItemSource::Lavfi {
                        params: format!(
                            "color=c=black:s={}x{}",
                            self.channel_config
                                .normalization
                                .video
                                .height
                                .unwrap_or(1920),
                            self.channel_config
                                .normalization
                                .video
                                .height
                                .unwrap_or(1080),
                        ),
                    },
                }),
            }),
        }
    }
}
