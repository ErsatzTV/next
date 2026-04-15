mod error;
mod generate;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use ffpipeline::probe::ProbeResult;
use walkdir::DirEntry;

use crate::error::PlayoutGeneratorError;

static VIDEO_EXTENSIONS: &[&str] = &[
    "avs", "mpg", "mp2", "mpeg", "mpe", "mpv", "ogg", "ogv", "mp4", "m4p", "m4v", "avi", "wmv",
    "mov", "mkv", "m2ts", "ts", "webm",
];

static IMAGE_EXTENSIONS: &[&str] = &["png"];

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, subcommand_negates_reqs = true)]
struct Args {
    #[arg(short, long, required = true)]
    content_folder: Option<PathBuf>,
    #[arg(short, long, required = true)]
    output_folder: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Sync {
        #[arg(short, long, required = true)]
        database: PathBuf,
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

async fn run() -> Result<(), PlayoutGeneratorError> {
    let args = Args::parse();

    match args.command {
        Some(Commands::Sync { database }) => {
            log::info!("synchronizing from database at {:?}", database);
            Ok(())
        }
        None => {
            let content_folder = args.content_folder.unwrap();
            let output_folder = args.output_folder.unwrap();
            generate::generate_playout(&content_folder, &output_folder).await
        }
    }
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

fn to_probe_result(dir_entry: DirEntry) -> Option<PathAndProbe> {
    if let Some(video_path) = dir_entry.path().to_str()
        && let Ok(probe_result) = ffpipeline::probe::probe(Path::new("ffprobe"), video_path)
    // && probe_result
    //     .duration
    //     .filter(|d| d.as_secs() < 120)
    //     .is_some()
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
