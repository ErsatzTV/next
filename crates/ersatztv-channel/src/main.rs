mod channel_session;
mod dossier;
mod fallback;
mod local_proxy;
mod playlist_manager;
mod playout_loader;
mod pts_scanner;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use ersatztv_channel::config::ChannelConfig;
use ersatztv_channel::error::ChannelError;
use ffpipeline::ffmpeg_info::FfmpegInfo;

use crate::channel_session::ChannelSession;

const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Parser, Debug)]
#[command(version = ersatztv_core::VERSION, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Print debug information using the provided configuration
    Debug {
        #[arg(required = true, num_args = 1..)]
        config_paths: Vec<PathBuf>,
    },
    /// Run the channel using the provided configuration
    Run {
        #[arg(required = true, num_args = 1..)]
        config_paths: Vec<PathBuf>,
        #[arg(short, long)]
        output_folder: PathBuf,
        #[arg(short, long)]
        number: String,
        #[arg(short, long)]
        troubleshoot: bool,
    },
}

pub fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            log::error!("failed to start runtime: {err}");
            return ExitCode::FAILURE;
        }
    };

    let result = runtime.block_on(run_until_shutdown());

    // process::exit skips drops, so kill_on_drop children would outlive us
    runtime.shutdown_timeout(SHUTDOWN_DEADLINE);

    match result {
        Ok(()) => ExitCode::SUCCESS,
        // no viewers is not a failure; supervisors treat non-zero as a crash
        Err(err @ ChannelError::IdleTimeout(_)) => {
            log::info!("{err}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            log::error!("{err}");
            ExitCode::FAILURE
        }
    }
}

async fn run_until_shutdown() -> Result<(), ChannelError> {
    // tokio never restores the default signal action, so a second SIGTERM can't kill us;
    // shutdown_timeout is the backstop
    tokio::select! {
        result = run() => result,
        signal = shutdown_signal() => {
            // dropping run() killed ffmpeg
            log::info!("received {signal}; shutting down");
            Ok(())
        }
    }
}

#[cfg(unix)]
async fn shutdown_signal() -> &'static str {
    use tokio::signal::unix::{SignalKind, signal};

    let (Ok(mut terminate), Ok(mut interrupt)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) else {
        log::warn!("failed to install signal handlers");
        return std::future::pending().await;
    };

    tokio::select! {
        _ = terminate.recv() => "SIGTERM",
        _ = interrupt.recv() => "SIGINT",
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() -> &'static str {
    if tokio::signal::ctrl_c().await.is_err() {
        log::warn!("failed to install ctrl+c handler");
        return std::future::pending().await;
    }

    "ctrl+c"
}

async fn run() -> Result<(), ChannelError> {
    let args = Args::parse();

    match args.command {
        Commands::Run {
            config_paths,
            output_folder,
            number,
            troubleshoot,
        } => {
            let channel_config =
                ChannelConfig::from_sources(&config_paths, &output_folder, &number).await?;

            // start channel session
            let mut channel_session = ChannelSession::new(channel_config).await?;
            channel_session.run(troubleshoot).await
        }
        Commands::Debug { config_paths } => {
            let channel_config =
                ChannelConfig::from_sources(&config_paths, &std::env::temp_dir(), "debug").await?;

            log::debug!("{:?}", channel_config);

            let ffmpeg_path = channel_config
                .ffmpeg
                .ffmpeg_path
                .as_deref()
                .unwrap_or(Path::new("ffmpeg"));
            let ffmpeg_info = FfmpegInfo::load(
                ffmpeg_path,
                &channel_config.ffmpeg.disabled_filters,
                &channel_config.ffmpeg.preferred_filters,
            )
            .await?;

            log::debug!("{:?}", ffmpeg_info);

            if let Some(accel) = &channel_config.normalization.video.accel {
                let _ = accel.to_pipeline(&channel_config);
            }

            Ok(())
        }
    }
}
