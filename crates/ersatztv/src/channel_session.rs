use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ersatztv::error::LineupError;
use ersatztv_core::{HEARTBEAT_FILE_NAME, READY_FILE_NAME, SHUTDOWN_DEADLINE};
use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::channel_model::{ChannelModel, StreamHints};

pub struct ChannelSession {
    ready_receiver: watch::Receiver<bool>,
    stream_hints: StreamHints,
}

impl ChannelSession {
    pub fn spawn(
        channel: &ChannelModel,
        active: Arc<Mutex<HashMap<String, ChannelSession>>>,
        shutdown: CancellationToken,
        tracker: &TaskTracker,
    ) -> Result<Self, LineupError> {
        let stream_hints = channel.stream_hints();
        let mut child = ersatztv_core::process::command(channel_binary_path()?)
            .arg("run")
            .arg("--output-folder")
            .arg(channel.output_folder())
            .arg("--number")
            .arg(channel.number())
            .arg(channel.config_path())
            .args(channel.overlay_paths())
            .spawn()
            .map_err(LineupError::Io)?;

        let (ready_sender, ready_receiver) = watch::channel(false);
        let ready_file = channel.output_folder().join(READY_FILE_NAME);
        let heartbeat_file = channel.output_folder().join(HEARTBEAT_FILE_NAME);
        let channel_number = channel.number().to_owned();

        tracker.spawn(async move {
            let ready_file_clone = ready_file.clone();
            let watcher = tokio::spawn(async move {
                loop {
                    if tokio::fs::metadata(&ready_file_clone).await.is_ok() {
                        let _ = ready_sender.send(true);
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            });

            tokio::select! {
                _ = child.wait() => {}
                _ = shutdown.cancelled() => {
                    log::debug!("stopping channel {}", channel_number);
                    let _ = ersatztv_core::process::stop(&mut child, SHUTDOWN_DEADLINE).await;
                }
            }
            watcher.abort();
            log::debug!("channel {} exited", channel_number);
            active.lock().await.remove(&channel_number);

            if ready_file.exists() {
                let _ = tokio::fs::remove_file(&ready_file).await;
            }

            if heartbeat_file.exists() {
                let _ = tokio::fs::remove_file(&heartbeat_file).await;
            }
        });

        Ok(ChannelSession {
            ready_receiver,
            stream_hints,
        })
    }

    pub fn subscribe_ready(&self) -> watch::Receiver<bool> {
        self.ready_receiver.clone()
    }

    pub fn stream_hints(&self) -> StreamHints {
        self.stream_hints
    }
}

fn channel_binary_path() -> Result<PathBuf, LineupError> {
    let mut path = std::env::current_exe()?
        .parent()
        .ok_or(LineupError::ChannelBinaryNotFound)?
        .to_path_buf();
    path.push(format!("ersatztv-channel{}", std::env::consts::EXE_SUFFIX));

    if path.is_file() {
        Ok(path)
    } else {
        Err(LineupError::ChannelBinaryNotFoundAtPath(
            path.to_string_lossy().to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use axum::extract::{Path, State};
    use axum::response::IntoResponse;

    use super::*;

    #[tokio::test]
    async fn playlists_keep_session_hints_until_the_next_session() {
        for (initial, updated) in [("burn", "convert"), ("convert", "burn")] {
            for use_overlay in [false, true] {
                let folder = tempfile::tempdir().unwrap();
                let config = |mode, bitrate| {
                    serde_json::json!({"normalization": {
                        "subtitle": {"mode": mode}, "video": {"bitrate_kbps": bitrate}
                    }})
                    .to_string()
                };
                std::fs::write(folder.path().join("channel.json"), config(initial, 4000)).unwrap();
                let edit_path = folder.path().join(if use_overlay {
                    "overlay.json"
                } else {
                    "channel.json"
                });
                std::fs::write(&edit_path, config(initial, 4000)).unwrap();
                let channel = ChannelModel::new(
                    &folder.path().join("lineup.json"),
                    folder.path(),
                    ersatztv::config::ChannelConfig {
                        number: "1".into(),
                        name: "One".into(),
                        config: "channel.json".into(),
                        overlays: if use_overlay {
                            vec!["overlay.json".into()]
                        } else {
                            vec![]
                        },
                        tvg_id: None,
                        logo: None,
                        group: None,
                    },
                )
                .unwrap();
                let (_ready_sender, ready_receiver) = watch::channel(true);
                let session = ChannelSession {
                    stream_hints: channel.stream_hints(),
                    ready_receiver: ready_receiver.clone(),
                };
                let state = Arc::new(crate::LineupState {
                    channels: vec![channel],
                    xmltv_folder: None,
                    active: Arc::new(Mutex::new(HashMap::from([("1".into(), session)]))),
                    shutdown: CancellationToken::new(),
                    tracker: TaskTracker::new(),
                });

                std::fs::write(&edit_path, config(updated, 8000)).unwrap();
                for (mode, bitrate) in [(initial, 4000), (updated, 8000)] {
                    let response = crate::stream(
                        Path("1.m3u8".into()),
                        State(Arc::clone(&state)),
                        axum::http::Request::new(axum::body::Body::empty()),
                    )
                    .await
                    .unwrap()
                    .into_response();
                    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                        .await
                        .unwrap();
                    let playlist = std::str::from_utf8(&body).unwrap();
                    assert_eq!(playlist.contains("TYPE=SUBTITLES"), mode == "convert");
                    assert!(playlist.contains(&format!("BANDWIDTH={}", (bitrate + 192) * 1100)));

                    state.active.lock().await.insert(
                        "1".into(),
                        ChannelSession {
                            stream_hints: state.channels[0].stream_hints(),
                            ready_receiver: ready_receiver.clone(),
                        },
                    );
                }
            }
        }
    }
}
