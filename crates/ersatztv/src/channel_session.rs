use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use ersatztv_core::{HEARTBEAT_FILE_NAME, READY_FILE_NAME, READY_FILE_TIMEOUT, wait_for_file};
use tokio::sync::{Mutex, watch};

use crate::channel_model::ChannelModel;
use crate::error::LineupError;

pub struct ChannelSession {
    ready_rx: watch::Receiver<bool>,
    kill_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl ChannelSession {
    pub fn spawn(
        channel: &ChannelModel,
        active: Arc<Mutex<HashMap<String, ChannelSession>>>,
    ) -> Result<Self, LineupError> {
        let mut child = tokio::process::Command::new(channel_binary_path()?)
            .arg("run")
            .arg("--output-folder")
            .arg(channel.output_folder())
            .arg("--number")
            .arg(channel.number())
            .arg(channel.config_path())
            .kill_on_drop(true)
            .spawn()
            .map_err(LineupError::Io)?;

        let (ready_tx, ready_rx) = watch::channel(false);
        let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();

        let ready_file = channel.output_folder().join(READY_FILE_NAME);
        tokio::spawn(async move {
            if wait_for_file(&ready_file, READY_FILE_TIMEOUT).await {
                let _ = ready_tx.send(true);
            }
        });

        let channel_number = channel.number().to_owned();
        let ready_file = channel.output_folder().join(READY_FILE_NAME);
        let heartbeat_file = channel.output_folder().join(HEARTBEAT_FILE_NAME);
        tokio::spawn(async move {
            tokio::select! {
                _ = child.wait() => {}
                _ = kill_rx => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                }
            }
            log::debug!("channel {} exited", &channel_number);
            active.lock().await.remove(&channel_number);

            if ready_file.exists() {
                let _ = tokio::fs::remove_file(&ready_file).await;
            }

            if heartbeat_file.exists() {
                let _ = tokio::fs::remove_file(&heartbeat_file).await;
            }
        });

        Ok(ChannelSession {
            ready_rx,
            kill_tx: Some(kill_tx),
        })
    }

    pub fn request_shutdown(&mut self) {
        if let Some(tx) = self.kill_tx.take() {
            let _ = tx.send(());
        }
    }

    pub fn subscribe_ready(&self) -> watch::Receiver<bool> {
        self.ready_rx.clone()
    }
}

fn channel_binary_path() -> Result<PathBuf, LineupError> {
    let mut path = std::env::current_exe()?
        .parent()
        .ok_or(LineupError::ChannelNotFound(String::from(
            "unable to locate channel binary",
        )))?
        .to_path_buf();
    path.push("ersatztv-channel");
    Ok(path)
}
