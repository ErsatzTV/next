use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use ersatztv_channel::error::ChannelError;
use futures_core::Stream;
use tokio::net::TcpListener;
use tokio::process::Command;

#[derive(Clone, Hash)]
pub struct ScriptCommand {
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Clone)]
struct ServerState {
    registry: Arc<Mutex<HashMap<String, ScriptCommand>>>,
}

pub struct LocalProxyServer {
    base: String,
    state: ServerState,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for LocalProxyServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl LocalProxyServer {
    pub async fn start() -> Result<Self, ChannelError> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let state = ServerState {
            registry: Default::default(),
        };
        let app = Router::new()
            .route("/{token}", get(handle))
            .with_state(state.clone());
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self {
            base: format!("http://127.0.0.1:{port}"),
            state,
            task,
        })
    }

    pub fn register_script(&self, cmd: ScriptCommand) -> Result<String, ChannelError> {
        let mut hasher = DefaultHasher::new();
        cmd.hash(&mut hasher);
        let token = format!("{:x}", hasher.finish());

        let mut registry = self.state.registry.lock().map_err(|e| {
            ChannelError::StreamFailure(format!("script registry lock is poisoned: {e}"))
        })?;

        registry.entry(token.clone()).or_insert(cmd);
        Ok(format!("{}/{token}", self.base))
    }
}

struct ProcStream {
    inner: tokio_util::io::ReaderStream<tokio::process::ChildStdout>,
    _child: tokio::process::Child,
}

impl Stream for ProcStream {
    type Item = std::io::Result<Bytes>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

async fn handle(State(st): State<ServerState>, Path(token): Path<String>) -> Response {
    let cmd = {
        let registry = match st.registry.lock() {
            Ok(r) => r,
            Err(e) => {
                log::error!("script registry lock is poisoned: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

        let Some(cmd) = registry.get(&token).cloned() else {
            return StatusCode::NOT_FOUND.into_response();
        };

        cmd
    };

    let mut child = match Command::new(&cmd.command)
        .args(&cmd.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("script spawn failed: {e}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    let Some(stdout) = child.stdout.take() else {
        log::error!("failed to capture stdout from script command");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let body = Body::from_stream(ProcStream {
        inner: tokio_util::io::ReaderStream::new(stdout),
        _child: child,
    });
    ([(CONTENT_TYPE, "video/mp2t")], body).into_response()
}
