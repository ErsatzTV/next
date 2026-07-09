mod channel_model;
mod channel_session;
mod scaffold;
mod xmltv;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use clap::{Parser, Subcommand};
use ersatztv::error::LineupError;
use ersatztv_core::{HEARTBEAT_FILE_NAME, READY_FILE_TIMEOUT, empty_folder};
use tokio::signal;
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use crate::channel_model::ChannelModel;
use crate::channel_session::ChannelSession;

#[derive(Parser, Debug)]
#[command(version = ersatztv_core::VERSION, about, long_about = None, subcommand_negates_reqs = true)]
struct Args {
    /// Path to lineup.json (server mode)
    #[arg(required = true)]
    lineup_path: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Scaffold a new lineup at the provided lineup.json path
    AddLineup {
        lineup_path: PathBuf,
        #[arg(long)]
        channels: u32,
        #[arg(long)]
        force: bool,
    },
    /// Add a channel to an existing lineup
    AddChannel {
        lineup_path: PathBuf,
        #[arg(long)]
        number: String,
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
pub async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    if let Err(err) = run().await {
        log::error!("{err}");
        std::process::exit(1);
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install ctrl+c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

async fn run() -> Result<(), LineupError> {
    let args = Args::parse();

    match args.command {
        Some(Commands::AddLineup {
            lineup_path,
            channels,
            force,
        }) => scaffold::add_lineup(&lineup_path, channels, force).await,
        Some(Commands::AddChannel {
            lineup_path,
            number,
            force,
        }) => scaffold::add_channel(&lineup_path, &number, force).await,
        None => {
            let lineup_path =
                args.lineup_path
                    .ok_or(LineupError::LineupConfigFailure(String::from(
                        "lineup path is required",
                    )))?;

            // load lineup config
            let lineup_config = ersatztv::config::from_file(&lineup_path).await?;
            let output_folder = PathBuf::from(&lineup_config.output.folder);

            let mut channels: Vec<ChannelModel> = Vec::with_capacity(lineup_config.channels.len());
            for channel in lineup_config.channels {
                match ChannelModel::new(&lineup_path, &output_folder, channel) {
                    Ok(channel_config) => {
                        channels.push(channel_config);
                    }
                    Err(err) => {
                        log::warn!("{err}")
                    }
                }
            }

            if channels.is_empty() {
                return Err(LineupError::NoChannelsLoaded);
            }

            log::debug!("loaded {} channel definitions", channels.len());

            empty_folder(&output_folder).await?;

            let state = Arc::new(LineupState {
                channels,
                xmltv_folder: lineup_config.xmltv.map(|c| c.folder),
                active: Arc::new(Mutex::new(HashMap::new())),
            });

            let addr = format!(
                "{}:{}",
                lineup_config.server.bind_address, lineup_config.server.port
            );

            let listener = tokio::net::TcpListener::bind(addr).await?;

            let app = Router::new()
                .route("/channel/{filename}", get(stream))
                .route("/channels.m3u", get(channel_playlist))
                .route("/xmltv.xml", get(xmltv))
                .nest_service(
                    "/session",
                    ServiceBuilder::new()
                        .layer(axum::middleware::from_fn_with_state(
                            Arc::clone(&state),
                            session_middleware,
                        ))
                        .service(tower_http::services::ServeDir::new(&output_folder)),
                )
                .layer(axum::middleware::from_fn(fix_content_types))
                .layer(CorsLayer::permissive())
                .with_state(state);

            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
                .await?;

            Ok(())
        }
    }
}

async fn stream(
    Path(filename): Path<String>,
    State(state): State<Arc<LineupState>>,
    request: axum::extract::Request,
) -> Result<impl IntoResponse, LineupError> {
    let number = filename
        .strip_suffix(".m3u8")
        .ok_or(LineupError::ChannelNotFound(filename.clone()))?;

    let channel = state
        .channels
        .iter()
        .find(|c| c.number() == number)
        .ok_or(LineupError::ChannelNotFound(number.to_owned()))?;

    let mut ready_receiver = {
        let mut active = state.active.lock().await;

        if let Some(channel_session) = active.get(number) {
            channel_session.subscribe_ready()
        } else {
            let channel_session = ChannelSession::spawn(channel, Arc::clone(&state.active))?;
            let ready_receiver = channel_session.subscribe_ready();
            active.insert(number.to_owned(), channel_session);
            ready_receiver
        }
    };

    let wait = ready_receiver.wait_for(|&r| r);
    match tokio::time::timeout(READY_FILE_TIMEOUT, wait).await {
        Ok(Ok(_)) => {}
        Ok(Err(_)) => return Err(LineupError::ChannelNotReady), // child died
        Err(_) => return Err(LineupError::ChannelNotReady),     // 30s deadline
    }

    let content = get_multi_variant(channel, request);

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "application/vnd.apple.mpegurl",
        )],
        content,
    ))
}

struct LineupState {
    channels: Vec<ChannelModel>,
    xmltv_folder: Option<String>,
    active: Arc<Mutex<HashMap<String, ChannelSession>>>,
}

async fn fix_content_types(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let is_m3u8 = request.uri().path().ends_with(".m3u8");
    let mut response = next.run(request).await;
    if is_m3u8 && let Ok(value) = "application/vnd.apple.mpegurl".parse() {
        response
            .headers_mut()
            .insert(axum::http::header::CONTENT_TYPE, value);
    }
    response
}

fn get_multi_variant(channel: &ChannelModel, request: axum::extract::Request) -> String {
    let mut result = String::new();
    result.push_str("#EXTM3U\n");
    result.push_str("#EXT-X-VERSION:6\n");
    result.push_str(&format!(
        "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"English\",DEFAULT=YES,AUTOSELECT=YES,FORCED=NO,LANGUAGE=\"en\",URI=\"{}/session/{}/live_sub.m3u8\"\n",
        get_scheme_host(&request),
        channel.number()
    ));
    result.push_str(&format!(
        "#EXT-X-STREAM-INF:BANDWIDTH={},SUBTITLES=\"subs\"\n",
        channel.bandwidth_bps()
    ));
    result.push_str(&format!(
        "{}/session/{}/live.m3u8",
        get_scheme_host(&request),
        channel.number()
    ));

    result
}

async fn channel_playlist(
    State(state): State<Arc<LineupState>>,
    request: axum::extract::Request,
) -> Result<impl IntoResponse, LineupError> {
    let mut content = String::new();
    let xmltv_url = format!("{}/xmltv.xml", get_scheme_host(&request));
    content.push_str(&format!(
        "#EXTM3U url-tvg=\"{xmltv_url}\" x-tvg-url=\"{xmltv_url}\"\n"
    ));
    for channel in &state.channels {
        let logo = channel
            .logo()
            .map(|l| format!(" tvg-logo=\"{l}\""))
            .unwrap_or(String::new());

        let group = channel
            .group()
            .map(|g| format!(" group-title=\"{g}\""))
            .unwrap_or(String::new());

        // TODO: kodiprop when user agent starts with "kodi"
        content.push_str(&format!(
            "#EXTINF:0 tvg-id=\"{}\" tvg-name=\"{}\"{}{}, {}\n",
            channel.tvg_id(),
            channel.name(),
            logo,
            group,
            channel.name()
        ));
        content.push_str(&format!(
            "{}/channel/{}.m3u8\n",
            get_scheme_host(&request),
            channel.number()
        ));
    }

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/x-mpegurl")],
        content,
    ))
}

async fn xmltv(
    State(state): State<Arc<LineupState>>,
    _request: axum::extract::Request,
) -> Result<impl IntoResponse, LineupError> {
    let content = xmltv::generate(&state).await?;
    Ok((
        [(axum::http::header::CONTENT_TYPE, "text/xml; charset=utf-8")],
        content,
    ))
}

fn get_scheme_host(request: &axum::extract::Request) -> String {
    // TODO: need scheme, host from reverse proxy
    let host = request
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");

    format!("http://{host}")
}

async fn session_middleware(
    State(state): State<Arc<LineupState>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // touch heartbeat file for channel
    let path = request.uri().path();
    if path.ends_with(".ts") || path.ends_with(".m3u8") {
        let split: Vec<&str> = request.uri().path().split('/').collect();
        let active = state.active.lock().await;
        if active.contains_key(split[1]) {
            let channel_number = split[1];
            if let Some(channel_config) =
                state.channels.iter().find(|c| c.number() == channel_number)
            {
                let heartbeat_file = channel_config.output_folder().join(HEARTBEAT_FILE_NAME);

                let mut exists = heartbeat_file.exists();
                if !exists {
                    exists = tokio::fs::write(&heartbeat_file, b"").await.is_ok();
                }

                if exists {
                    let _ = filetime::set_file_mtime(&heartbeat_file, filetime::FileTime::now());
                }
            }
        }
    }

    next.run(request).await
}
