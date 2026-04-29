mod channel_session;
mod playlist_manager;
mod playout_loader;
mod pts_scanner;
mod web_vtt;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use ersatztv_channel::config::ChannelConfig;
use ersatztv_channel::error::ChannelError;
use ffpipeline::ffmpeg_info::FfmpegInfo;

use crate::channel_session::ChannelSession;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Print debug information using the provided configuration
    Debug { config_path: PathBuf },
    /// Run the channel using the provided configuration
    Run {
        config_path: PathBuf,
        #[arg(short, long)]
        output_folder: PathBuf,
        #[arg(short, long)]
        number: String,
    },
}

#[tokio::main]
pub async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    if let Err(err) = run().await {
        match err {
            ChannelError::IdleTimeout(_) => log::info!("{err}"),
            _ => log::error!("{err}"),
        };

        std::process::exit(1);
    }
}

async fn run() -> Result<(), ChannelError> {
    let args = Args::parse();

    match args.command {
        Commands::Run {
            config_path,
            output_folder,
            number,
        } => {
            let channel_config = if config_path.to_str().is_some_and(|p| p == "-") {
                ChannelConfig::from_stdin(&output_folder, &number).await?
            } else {
                ChannelConfig::from_file(&config_path, &output_folder, &number).await?
            };

            // start channel session
            let mut channel_session = ChannelSession::new(channel_config)?;
            channel_session.run().await
        }
        Commands::Debug { config_path } => {
            let channel_config =
                ChannelConfig::from_file(&config_path, &std::env::temp_dir(), "debug").await?;

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
