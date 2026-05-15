mod error;
mod generate;
mod xmltv;

use std::path::{Path, PathBuf};

use clap::{ArgGroup, Parser};
use ffpipeline::input::{InputSource, LocalInputSource};
use ffpipeline::probe;
use ffpipeline::probe::{ProbeResult, Probeable};
use serde::Deserialize;
use walkdir::DirEntry;

use crate::error::PlayoutGeneratorError;

static VIDEO_EXTENSIONS: &[&str] = &[
    "avs", "mpg", "mp2", "mpeg", "mpe", "mpv", "ogg", "ogv", "mp4", "m4p", "m4v", "avi", "wmv",
    "mov", "mkv", "m2ts", "ts", "webm",
];

static IMAGE_EXTENSIONS: &[&str] = &["png"];

const PATH_FIELDS: &[&str] = &["/playout/folder"];

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(group(ArgGroup::new("target").required(true).args(["lineup", "output_folder"])))]
struct Args {
    #[arg(short, long, required = true)]
    content_folder: Option<PathBuf>,

    /// Resolve the output folder from a lineup.json and channel number
    #[arg(long, requires = "channel")]
    lineup: Option<PathBuf>,
    #[arg(long, requires = "lineup")]
    channel: Option<String>,

    /// Or write directly to this folder
    #[arg(short, long, required = true)]
    output_folder: Option<PathBuf>,
}

struct GeneratorConfig {
    pub output_folder: PathBuf,
    pub xmltv_folder: Option<PathBuf>,
    pub channel_tvg_id: Option<String>,
}

#[tokio::main]
pub async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("sqlx", log::LevelFilter::Warn)
        .init();

    if let Err(err) = run().await {
        log::error!("{err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), PlayoutGeneratorError> {
    let args = Args::parse();
    let content_folder = args.content_folder.as_ref().unwrap();
    let generator_config = resolve_output_folder(&args).await?;
    generate::generate_playout(
        content_folder,
        &generator_config.output_folder,
        generator_config.xmltv_folder.as_deref(),
        generator_config.channel_tvg_id.as_deref(),
    )
    .await
}

async fn resolve_output_folder(args: &Args) -> Result<GeneratorConfig, PlayoutGeneratorError> {
    if let Some(lineup) = args.lineup.as_ref()
        && let Some(number) = args.channel.as_ref()
    {
        let lineup_parent = lineup
            .parent()
            .ok_or(PlayoutGeneratorError::LineupNoParent)?;
        let lineup_config = ersatztv::config::from_file(lineup).await?;
        if let Some(channel) = lineup_config.channels.iter().find(|c| &c.number == number) {
            let channel_config_file =
                ersatztv::validate_config_path(lineup_parent, &channel.config)?;

            let base_parent =
                channel_config_file
                    .parent()
                    .ok_or(PlayoutGeneratorError::ChannelNoParent(
                        channel_config_file.to_string_lossy().to_string(),
                    ))?;
            let base = tokio::fs::read_to_string(channel_config_file.clone()).await?;
            let mut merged: serde_json::Value = serde_json::from_str(&base)?;
            ersatztv_core::resolve_relative_paths(&mut merged, base_parent, PATH_FIELDS);

            for overlay_rel in &channel.overlays {
                let overlay_path = ersatztv::validate_config_path(lineup_parent, overlay_rel)?;
                let overlay_parent =
                    overlay_path
                        .parent()
                        .ok_or(PlayoutGeneratorError::ChannelNoParent(
                            overlay_path.to_string_lossy().to_string(),
                        ))?;
                let overlay_str = tokio::fs::read_to_string(&overlay_path).await?;
                let mut overlay_value: serde_json::Value = serde_json::from_str(&overlay_str)?;
                ersatztv_core::resolve_relative_paths(
                    &mut overlay_value,
                    overlay_parent,
                    PATH_FIELDS,
                );
                ersatztv_core::deep_merge(&mut merged, overlay_value);
            }

            let channel_config: MinimalChannelConfig = serde_json::from_value(merged)
                .map_err(|e| PlayoutGeneratorError::ChannelJsonLoadError(e.to_string()))?;

            return Ok(GeneratorConfig {
                output_folder: PathBuf::from(channel_config.playout.folder),
                xmltv_folder: lineup_config.xmltv.map(|x| PathBuf::from(x.folder)),
                channel_tvg_id: Some(
                    channel
                        .tvg_id
                        .as_ref()
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| format!("ersatztv.{}", channel.number)),
                ),
            });
        }

        return Err(PlayoutGeneratorError::LineupNoChannel);
    }

    args.output_folder
        .as_ref()
        .map(|p| GeneratorConfig {
            output_folder: p.clone(),
            xmltv_folder: None,
            channel_tvg_id: None,
        })
        .ok_or(PlayoutGeneratorError::NoOutputFolder)
}

fn is_video_extension(dir_entry: &DirEntry) -> bool {
    if let Some(extension) = dir_entry.path().extension()
        && let Some(extension) = extension.to_str()
    {
        return VIDEO_EXTENSIONS.contains(&extension);
    }

    false
}

fn is_image_extension(dir_entry: &DirEntry) -> bool {
    if let Some(extension) = dir_entry.path().extension()
        && let Some(extension) = extension.to_str()
    {
        return IMAGE_EXTENSIONS.contains(&extension);
    }

    false
}

async fn to_probe_result(dir_entry: &DirEntry) -> Option<PathAndProbe> {
    if let Some(video_path) = dir_entry.path().to_str()
        && let input_source = InputSource::Local(LocalInputSource {
            path: video_path.to_string(),
        })
        && let Ok(probe_result) = input_source
            .probe(&probe::ProbeDeps {
                ffprobe_path: Path::new("ffprobe"),
                ffmpeg_path: Path::new("ffmpeg"),
            })
            .await
    {
        return Some(PathAndProbe {
            path: dir_entry.path().to_path_buf(),
            probe: probe_result,
        });
    }

    None
}

#[derive(Debug)]
struct PathAndProbe {
    path: PathBuf,
    probe: ProbeResult,
}

#[derive(Deserialize, Clone, Debug)]
struct MinimalChannelConfig {
    playout: MinimalChannelPlayoutConfig,
}

#[derive(Deserialize, Clone, Debug)]
struct MinimalChannelPlayoutConfig {
    folder: String,
}
