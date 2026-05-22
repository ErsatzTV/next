use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ersatztv_channel::config::ChannelConfig;
use ersatztv_channel::error::ChannelError;
use ersatztv_core::{READY_FILE_NAME, empty_folder};
use ersatztv_playout::playout::{
    PeriodicClock, PlayoutItem, PlayoutItemSource, PlayoutItemTracks, TrackSelection,
    WatermarkLocation, WatermarkTiming,
};
use ersatztv_playout::template::expand_template;
use ffpipeline::ffmpeg_info::FfmpegInfo;
use ffpipeline::frame_rate::FrameRate;
use ffpipeline::frame_size::FrameSize;
use ffpipeline::input::{
    HttpInputOptions, HttpInputSource, InputSettings, InputSource, LavfiInputSource,
    LocalInputSource, ProbedInput, RtspInputOptions, RtspInputSource, WatermarkInput,
};
use ffpipeline::output_settings::{AudioOutputSettings, OutputSettings};
use ffpipeline::pipeline::{AudioFormat, Hz, Kbps, PtsOffset, SEGMENT_SECONDS, VideoFormat};
use ffpipeline::probe::{ProbeResult, ProbeResultVideoStream, Probeable};
use ffpipeline::web_vtt::Cue;
use ffpipeline::{pipeline, probe};
use time::OffsetDateTime;
use tokio::io::AsyncBufReadExt;
use tokio::sync::Mutex;

use crate::dossier::Dossier;
use crate::local_proxy::{LocalProxyServer, ScriptCommand};
use crate::playlist_manager::{PlaylistManager, PlaylistManagerOutputFiles, SubtitleSource};
use crate::playout_loader::PlayoutLoader;
use crate::pts_scanner::{PtsScanner, PtsTime};

const STDERR_RING_LINES: usize = 2_000;

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
    local_proxy_server: LocalProxyServer,

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

    cached_subtitles: Option<(String, Arc<Vec<Cue>>)>,
}

impl ChannelSession {
    pub async fn new(channel_config: ChannelConfig) -> Result<ChannelSession, ChannelError> {
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

        let generated_subtitle_output_file = output_folder
            .join("live_sub.m3u8")
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
            PlaylistManagerOutputFiles {
                generated_playlist_file: generated_output_file,
                generated_subtitle_playlist_file: generated_subtitle_output_file,
                ffmpeg_playlist_file: ffmpeg_output_file.to_owned(),
            },
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

        let local_proxy_server = LocalProxyServer::start().await?;

        Ok(ChannelSession {
            channel_config,
            playout_loader,
            pts_scanner,
            playlist_manager,
            local_proxy_server,
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
            cached_subtitles: None,
        })
    }

    pub async fn run(&mut self) -> Result<(), ChannelError> {
        self.prep_output_folder().await?;

        self.ffmpeg_info = FfmpegInfo::load(
            &self.ffmpeg_path,
            &self.channel_config.ffmpeg.disabled_filters,
            &self.channel_config.ffmpeg.preferred_filters,
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

        let pts_duration = pts_time.map(|p| p.duration);

        let result = self
            .transcode_item(&current_item, realtime, pts_duration)
            .await;

        let (finish, is_complete) = match result {
            Ok(ok) => ok,
            Err(e @ ChannelError::IdleTimeout(_)) => return Err(e),
            Err(e) => {
                log::error!("item failed, replacing with black/silence: {e}");
                let fake_item = self.fake_playout_item(Some(current_item.finish));
                self.transcode_item(&fake_item, realtime, pts_duration)
                    .await?
            }
        };

        self.transcoded_until = finish;
        log::debug!("transcoded until: {}", self.transcoded_until);

        self.state = Self::next_state(self.state, is_complete);

        Ok(())
    }

    async fn transcode_item(
        &mut self,
        current_item: &PlayoutItem,
        realtime: bool,
        pts_duration: Option<Duration>,
    ) -> Result<(OffsetDateTime, bool), ChannelError> {
        // prioritize source from audio tracks, then default source
        let audio_source = Self::resolve_source(current_item, |t| t.audio.as_ref())
            .ok_or(ChannelError::PlayoutJsonAudioSourceRequired)?;

        // prioritize source from video tracks, then default source
        let video_source = Self::resolve_source(current_item, |t| t.video.as_ref())
            .ok_or(ChannelError::PlayoutJsonVideoSourceRequired)?;

        // prioritize source from subtitle tracks, then default source
        let subtitle_source = Self::resolve_source(current_item, |t| t.subtitle.as_ref());

        let audio_source_is_video_source = audio_source == video_source;
        let subtitle_source_is_video_source =
            subtitle_source.as_ref().is_some_and(|s| s == &video_source);

        let audio_input_source = self.playout_source_to_input_source(audio_source.clone())?;
        let video_input_source = if audio_source_is_video_source {
            audio_input_source.clone()
        } else {
            self.playout_source_to_input_source(video_source.clone())?
        };
        let subtitle_input_source = if subtitle_source_is_video_source {
            Some(video_input_source.clone())
        } else {
            subtitle_source
                .clone()
                .and_then(|s| self.playout_source_to_input_source(s.clone()).ok())
        };

        let audio_fut = self.probe_source(&audio_input_source);
        let video_fut = async {
            if audio_source_is_video_source {
                Ok::<_, ChannelError>(None)
            } else {
                self.probe_source(&video_input_source).await.map(Some)
            }
        };
        let subtitle_fut = async {
            if subtitle_source_is_video_source {
                Ok::<_, ChannelError>(None)
            } else if let Some(s) = subtitle_input_source.as_ref() {
                self.probe_source(s).await.map(Some)
            } else {
                Ok(None)
            }
        };

        let watermark_fut = async {
            if let Some(w) = current_item.watermark.as_ref() {
                let input_source = self.playout_source_to_input_source(w.source.clone())?;
                let location = playout_location_to_pipeline(&w.location);
                let timing = playout_timing_to_pipeline(w.timing.as_ref());

                let probe_result = self.probe_source(&input_source).await?;
                Ok(Some(WatermarkInput {
                    input_source,
                    probe_result,
                    stream_index: w.stream_index,
                    location,
                    width_percent: w.width_percent,
                    within_source_content: w.within_source_content,
                    horizontal_margin_percent: w.horizontal_margin_percent,
                    vertical_margin_percent: w.vertical_margin_percent,
                    opacity_percent: w.opacity_percent,
                    timing,
                }))
            } else {
                Ok(None)
            }
        };

        let (audio_probe_result, video_probe_opt, subtitle_probe_opt, watermark_input) =
            tokio::try_join!(audio_fut, video_fut, subtitle_fut, watermark_fut)?;

        let video_probe_result = video_probe_opt.unwrap_or_else(|| audio_probe_result.clone());
        let subtitle_probe_result = if subtitle_source_is_video_source {
            Some(video_probe_result.clone())
        } else {
            subtitle_probe_opt
        };

        let audio_norm = &self.channel_config.normalization.audio;
        let video_norm = &self.channel_config.normalization.video;

        let video_size = match (video_norm.width, video_norm.height) {
            (Some(width), Some(height)) => Some(FrameSize { width, height }),
            _ => None,
        };

        // consider an item to be live if any of its sources are live;
        // live sources can never seek or work ahead
        let is_live = source_is_live(&video_source) || source_is_live(&audio_source);

        // generate pipeline
        let output_settings = OutputSettings {
            audio: AudioOutputSettings {
                format: audio_norm.format.clone().map(AudioFormat::from),
                bitrate: audio_norm.bitrate_kbps.map(Kbps),
                buffer: audio_norm.buffer_kbps.map(Kbps),
                channels: audio_norm.channels,
                sample_rate: audio_norm.sample_rate_hz.map(Hz),
                loudness: if audio_norm.normalize_loudness {
                    Some(
                        audio_norm
                            .loudness
                            .as_ref()
                            .map(|l| l.into())
                            .unwrap_or_default(),
                    )
                } else {
                    None
                },
            },
            video_format: video_norm.format.clone().map(VideoFormat::from),
            bit_depth: video_norm.bit_depth,
            video_bitrate: video_norm.bitrate_kbps.map(Kbps),
            video_buffer: video_norm.buffer_kbps.map(Kbps),
            video_size,
            scaling_mode: video_norm.scaling_mode.into(),
            filter_options: video_norm.filters.clone().into(),
            deinterlace: video_norm.deinterlace,
            accel: self.hw_accel.clone(),
            format: ffpipeline::output_format::OutputFormat::Hls {
                playlist: self.output_file.clone(),
                segment_template: self.output_segment_template.clone(),
            },
            pts_offset: pts_duration.map(|duration| PtsOffset { duration }),
            realtime,
            is_live,
            frame_rate: if video_probe_result.is_still_image() {
                Some(FrameRate::default())
            } else {
                None
            },
            subtitle_mode: self.channel_config.normalization.subtitle.mode.into(),
            reports_folder: self.channel_config.ffmpeg.reports_folder.clone(),
            report_id: Some(self.channel_config.number().to_owned()),
        };

        let start_at_zero = matches!(
            self.state,
            ChannelSessionState::ZeroAndWorkAhead | ChannelSessionState::ZeroAndRealtime
        );

        let audio_timing = self.input_timing(
            current_item,
            &audio_source,
            start_at_zero,
            realtime,
            is_live,
        );
        let video_timing = self.input_timing(
            current_item,
            &video_source,
            start_at_zero,
            realtime,
            is_live,
        );
        let subtitle_timing = subtitle_source
            .as_ref()
            .map(|s| self.input_timing(current_item, s, start_at_zero, realtime, is_live));

        let video_index = current_item
            .tracks
            .as_ref()
            .and_then(|t| t.video.as_ref())
            .and_then(|v| v.stream_index);

        let audio_index = current_item
            .tracks
            .as_ref()
            .and_then(|t| t.audio.as_ref())
            .and_then(|a| a.stream_index);

        let subtitle_index = current_item
            .tracks
            .as_ref()
            .and_then(|t| t.subtitle.as_ref())
            .and_then(|s| s.stream_index);

        let subtitle_input = match (
            subtitle_probe_result,
            subtitle_input_source,
            subtitle_timing,
        ) {
            (Some(s_probe), Some(s_in), Some(s_time)) => Some(ProbedInput {
                input_source: s_in,
                in_point: s_time.in_point,
                out_point: s_time.out_point,
                probe_result: s_probe,
                stream_index: subtitle_index,
            }),
            _ => None,
        };

        let input_settings = InputSettings {
            start: current_item.start,
            audio_input: ProbedInput {
                input_source: audio_input_source,
                in_point: audio_timing.in_point,
                out_point: audio_timing.out_point,
                probe_result: audio_probe_result,
                stream_index: audio_index,
            },
            video_input: ProbedInput {
                input_source: video_input_source,
                in_point: if video_probe_result.is_still_image() {
                    Duration::ZERO
                } else {
                    video_timing.in_point
                },
                out_point: video_timing.out_point,
                probe_result: video_probe_result,
                stream_index: video_index,
            },
            subtitle_input,
            watermark_input,
        };

        let mut subtitle_source: Option<SubtitleSource> = None;
        if output_settings.subtitle_mode == ffpipeline::output_settings::SubtitleMode::Convert
            && let Some(subtitle_stream) = input_settings.select_subtitle_stream()
            && !subtitle_stream.is_subtitle_image()
            && let Some(input) = input_settings.subtitle_input.as_ref()
            && let Some(cues) = match &self.cached_subtitles {
                Some((id, c)) if id == &current_item.id => Some(Arc::clone(c)),
                _ => {
                    self.extract_and_convert_subs(input, subtitle_stream, current_item)
                        .await
                }
            }
        {
            subtitle_source = Some(SubtitleSource {
                cues,
                cursor: 0,
                next_segment_source_offset: input.in_point,
            });
        }

        let pts_offset = output_settings.pts_offset;
        let mut pipeline_result =
            pipeline::generate_pipeline(&self.ffmpeg_info, input_settings, output_settings)?;
        pipeline_result.optimize();
        let args = pipeline_result.args();
        let envs = pipeline_result.envs();
        log::debug!("optimized pipeline: {}", args.join(" "));

        self.playlist_manager
            .lock()
            .await
            .before_new_pipeline(pts_offset, subtitle_source)
            .await?;

        // stream current item
        let mut ffmpeg_child = tokio::process::Command::new(&self.ffmpeg_path)
            .args(args.iter().map(Cow::as_ref))
            .envs(
                envs.iter()
                    .map(|env| (env.key.as_str(), env.value.as_str())),
            )
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|_| ChannelError::StreamFailure(String::from("failed to spawn ffmpeg")))?;

        let stderr = ffmpeg_child
            .stderr
            .take()
            .ok_or(ChannelError::CaptureFFmpegStderrFailure)?;
        let ring = Arc::new(std::sync::Mutex::new(VecDeque::<String>::with_capacity(
            STDERR_RING_LINES,
        )));

        let reader_ring = Arc::clone(&ring);
        let reader_handle = tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("{line}");
                if let Ok(mut buf) = reader_ring.lock() {
                    if buf.len() == STDERR_RING_LINES {
                        buf.pop_front();
                    }
                    buf.push_back(line);
                }
            }
        });

        log::debug!("waiting for ffmpeg to terminate...");

        tokio::select! {
            status = ffmpeg_child.wait() => {
                let status = status.map_err(|e| ChannelError::StreamFailure(e.to_string()))?;
                let _ = reader_handle.await;
                if !status.success() {
                    let stderr_tail = ring
                        .lock()
                        .map(|r| r.iter().cloned().collect())
                        .unwrap_or_default();
                    let report_source_file = self.channel_config.ffmpeg.reports_folder.as_ref().map(|folder| {
                        PathBuf::from(folder).join(format!(".in-flight-{}.log", self.channel_config.number()))
                    });
                    let dossier = Dossier::new(&self.channel_config, current_item, stderr_tail, report_source_file);
                    if let Err(err) = dossier.write().await {
                        log::error!("failed to save dossier: {err}");
                    }
                    return Err(ChannelError::StreamFailure(format!(
                        "ffmpeg exited {status}"
                    )));
                } else {
                    self.cleanup_old_report().await;
                }
            }
            _ = self.timeout_notify.notified() => {
                ffmpeg_child.kill().await.ok();
                let _ = reader_handle.await;
                self.cleanup_old_report().await;
                return Err(ChannelError::IdleTimeout(self.channel_config.number().to_owned()));
            }
        }

        let finish = std::cmp::min(audio_timing.finish, video_timing.finish);
        let is_complete = is_live || (audio_timing.is_complete && video_timing.is_complete);

        Ok((finish, is_complete))
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

    async fn probe_source(&self, source: &InputSource) -> Result<ProbeResult, ChannelError> {
        let probe_deps = probe::ProbeDeps {
            ffprobe_path: &self.ffprobe_path,
            ffmpeg_path: &self.ffmpeg_path,
        };

        Ok(source.probe(&probe_deps).await?)
    }

    fn playout_source_to_input_source(
        &self,
        source: PlayoutItemSource,
    ) -> Result<InputSource, ChannelError> {
        match source {
            PlayoutItemSource::Local { path, .. } => {
                Ok(InputSource::Local(LocalInputSource { path }))
            }
            PlayoutItemSource::Lavfi { params } => {
                Ok(InputSource::Lavfi(LavfiInputSource { params }))
            }
            PlayoutItemSource::Http {
                uri,
                headers,
                user_agent,
                timeout_us,
                reconnect,
                reconnect_delay_max,
                keep_alive,
                ..
            } => {
                let expanded_uri = expand_template(&uri)?;
                let expanded_headers: Vec<String> = headers
                    .unwrap_or_default()
                    .iter()
                    .map(|h| expand_template(h))
                    .collect::<Result<Vec<_>, _>>()?;
                let expanded_ua = user_agent.as_deref().map(expand_template).transpose()?;

                Ok(InputSource::Http(HttpInputSource {
                    uri: expanded_uri,
                    options: HttpInputOptions {
                        headers: expanded_headers,
                        user_agent: expanded_ua,
                        timeout_us,
                        reconnect: reconnect.unwrap_or(true),
                        reconnect_delay_max,
                        keep_alive,
                    },
                }))
            }
            PlayoutItemSource::Rtsp { uri, timeout_us } => {
                let expanded_uri = expand_template(&uri)?;

                Ok(InputSource::Rtsp(RtspInputSource {
                    uri: expanded_uri,
                    options: RtspInputOptions { timeout_us },
                }))
            }
            PlayoutItemSource::Script { command, args, .. } => {
                let url = self.local_proxy_server.register_script(ScriptCommand {
                    command: expand_template(&command)?,
                    args: args
                        .iter()
                        .map(|a| expand_template(a))
                        .collect::<Result<_, _>>()?,
                })?;
                Ok(InputSource::Http(HttpInputSource {
                    uri: url,
                    options: HttpInputOptions {
                        reconnect: false,
                        ..Default::default()
                    },
                }))
            }
        }
    }

    fn input_timing(
        &self,
        current_item: &PlayoutItem,
        source: &PlayoutItemSource,
        start_at_zero: bool,
        realtime: bool,
        is_live: bool,
    ) -> TimingResult {
        let mut is_complete = true;

        let item_start = current_item.start;
        let item_finish = current_item.finish;
        let item_duration = current_item.finish - current_item.start;
        let item_in_point_base_ms = match source {
            PlayoutItemSource::Local { in_point_ms, .. }
            | PlayoutItemSource::Http { in_point_ms, .. } => in_point_ms.unwrap_or(0),
            _ => 0,
        };
        let item_out_point_ms = match source {
            PlayoutItemSource::Local { out_point_ms, .. }
            | PlayoutItemSource::Http { out_point_ms, .. } => out_point_ms
                .unwrap_or(item_in_point_base_ms + item_duration.whole_milliseconds() as u64),
            _ => item_in_point_base_ms + item_duration.whole_milliseconds() as u64,
        };

        // live content never seeks and is always a complete transcode
        if is_live {
            return TimingResult {
                in_point: Duration::ZERO,
                out_point: Duration::from_millis(item_duration.whole_milliseconds() as u64),
                finish: item_finish,
                is_complete: true,
            };
        }

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
                audio: Some(TrackSelection {
                    source: Some(PlayoutItemSource::Lavfi {
                        params: String::from("anullsrc=channel_layout=stereo:sample_rate=48000"),
                    }),
                    stream_index: None,
                }),
                video: Some(TrackSelection {
                    source: Some(PlayoutItemSource::Lavfi {
                        params: format!(
                            "color=c=black:s={}x{}",
                            self.channel_config
                                .normalization
                                .video
                                .width
                                .unwrap_or(1920),
                            self.channel_config
                                .normalization
                                .video
                                .height
                                .unwrap_or(1080),
                        ),
                    }),
                    stream_index: None,
                }),
                subtitle: None,
            }),
            watermark: None,
        }
    }

    fn resolve_source<F>(item: &PlayoutItem, pick: F) -> Option<PlayoutItemSource>
    where
        F: FnOnce(&PlayoutItemTracks) -> Option<&TrackSelection>,
    {
        item.tracks
            .as_ref()
            .and_then(pick)
            .and_then(|sel| sel.source.clone())
            .or_else(|| item.source.clone())
    }

    async fn extract_and_convert_subs(
        &mut self,
        input: &ProbedInput,
        subtitle_stream: &ProbeResultVideoStream,
        current_item: &PlayoutItem,
    ) -> Option<Arc<Vec<Cue>>> {
        {
            match ffpipeline::web_vtt::convert_to_vtt(&self.ffmpeg_path, input, subtitle_stream)
                .await
            {
                Ok(temp_file) => match ffpipeline::web_vtt::parse_file(temp_file.path()).await {
                    Ok(extracted_cues) => {
                        let arc = Arc::new(extracted_cues);
                        self.cached_subtitles = Some((current_item.id.clone(), Arc::clone(&arc)));
                        Some(arc)
                    }
                    Err(err) => {
                        log::warn!("error parsing converted vtt: {err}");
                        None
                    }
                },
                Err(err) => {
                    log::warn!("error converting subtitle to vtt: {err}");
                    None
                }
            }
        }
    }

    async fn cleanup_old_report(&self) {
        if let Some(reports_folder) = &self.channel_config.ffmpeg.reports_folder {
            let report_file = PathBuf::from(reports_folder)
                .join(format!(".in-flight-{}.log", self.channel_config.number()));
            if report_file.exists() {
                let _ = tokio::fs::remove_file(report_file).await;
            }
        }
    }
}

fn playout_location_to_pipeline(value: &WatermarkLocation) -> ffpipeline::input::WatermarkLocation {
    match value {
        WatermarkLocation::TopLeft => ffpipeline::input::WatermarkLocation::TopLeft,
        WatermarkLocation::TopCenter => ffpipeline::input::WatermarkLocation::TopCenter,
        WatermarkLocation::TopRight => ffpipeline::input::WatermarkLocation::TopRight,
        WatermarkLocation::CenterLeft => ffpipeline::input::WatermarkLocation::CenterLeft,
        WatermarkLocation::Center => ffpipeline::input::WatermarkLocation::Center,
        WatermarkLocation::CenterRight => ffpipeline::input::WatermarkLocation::CenterRight,
        WatermarkLocation::BottomLeft => ffpipeline::input::WatermarkLocation::BottomLeft,
        WatermarkLocation::BottomCenter => ffpipeline::input::WatermarkLocation::BottomCenter,
        WatermarkLocation::BottomRight => ffpipeline::input::WatermarkLocation::BottomRight,
    }
}

fn playout_timing_to_pipeline(
    value: Option<&WatermarkTiming>,
) -> Option<ffpipeline::input::WatermarkTiming> {
    value.map(|timing| {
        let WatermarkTiming::Periodic {
            clock,
            frequency_ms,
            phase_offset_ms,
            disable_after_ms,
            fade_ms,
            hold_ms,
        } = timing;

        let clock = match clock {
            PeriodicClock::Content => ffpipeline::input::PeriodicClock::Content,
            PeriodicClock::Wall => ffpipeline::input::PeriodicClock::Wall,
        };

        let periodic_timing = ffpipeline::input::PeriodicTiming {
            clock,
            frequency_ms: *frequency_ms,
            phase_offset_ms: *phase_offset_ms,
            disable_after_ms: *disable_after_ms,
            fade_ms: *fade_ms,
            hold_ms: *hold_ms,
        };

        ffpipeline::input::WatermarkTiming::Periodic(periodic_timing)
    })
}

fn source_is_live(source: &PlayoutItemSource) -> bool {
    matches!(
        source,
        PlayoutItemSource::Script {
            is_live: Some(true),
            ..
        } | PlayoutItemSource::Rtsp { .. }
    )
}
