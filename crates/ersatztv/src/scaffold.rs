use std::path::Path;

use ersatztv::config;
use ersatztv::config::{ChannelConfig, LineupConfig, OutputConfig, ServerConfig};
use ersatztv::error::LineupError;

pub async fn add_lineup(lineup_path: &Path, channels: u32, force: bool) -> Result<(), LineupError> {
    let root = lineup_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    let channel_numbers: Vec<_> = (0..channels).map(|i| format!("{}", i + 1)).collect();

    let mut folders_to_create = vec![root.join("hls")];

    let mut files_to_create = vec![lineup_path.to_path_buf()];

    for channel_number in channel_numbers.iter() {
        let channel_folder = root.join("channels").join(channel_number);
        folders_to_create.push(channel_folder.clone());
        folders_to_create.push(channel_folder.join("playout"));

        files_to_create.push(channel_folder.join("channel.json"));
    }

    let existing: Vec<_> = folders_to_create
        .iter()
        .chain(files_to_create.iter())
        .filter(|p| p.exists())
        .collect();

    if !existing.is_empty() {
        if !force {
            let names: Vec<_> = existing.iter().map(|p| p.to_string_lossy()).collect();
            return Err(LineupError::ScaffoldPathsExist(names.join(", ")));
        }

        for path in &existing {
            if path.is_dir() {
                tokio::fs::remove_dir_all(path).await?;
            } else {
                tokio::fs::remove_file(path).await?;
            }
        }
    }

    if !root.exists() {
        tokio::fs::create_dir_all(root).await?;
    }

    for folder in folders_to_create {
        tokio::fs::create_dir_all(folder).await?;
    }

    let mut channels_config = Vec::new();
    let channel_template = include_str!("templates/channel.json");
    for channel_number in channel_numbers.iter() {
        let channel_file = root
            .join("channels")
            .join(channel_number)
            .join("channel.json");

        tokio::fs::write(&channel_file, &channel_template).await?;
        channels_config.push(ChannelConfig::scaffold(channel_number))
    }

    let lineup = LineupConfig {
        server: ServerConfig {
            bind_address: String::from("0.0.0.0"),
            port: 8409,
        },
        output: OutputConfig {
            folder: String::from("./hls"),
        },
        channels: channels_config,
    };

    let lineup_json =
        serde_json::to_string_pretty(&lineup).map_err(LineupError::ScaffoldSerializeError)?;

    tokio::fs::write(lineup_path, &lineup_json).await?;

    Ok(())
}

pub async fn add_channel(lineup_path: &Path, number: &str, force: bool) -> Result<(), LineupError> {
    let root = lineup_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    let channel_folder = root.join("channels").join(number);
    let channel_file = channel_folder.join("channel.json");

    let lineup_config = config::from_file(&lineup_path.to_path_buf()).await?;

    let mut new_channels = lineup_config.channels;

    if new_channels.iter().any(|c| c.number == number) || channel_file.exists() {
        if !force {
            return Err(LineupError::ScaffoldChannelExists);
        }

        new_channels.retain(|c| c.number != number);
        if channel_folder.exists() {
            tokio::fs::remove_dir_all(channel_folder.clone()).await?;
        }
    }

    tokio::fs::create_dir_all(channel_folder.clone()).await?;
    tokio::fs::create_dir_all(channel_folder.join("playout")).await?;

    let channel_template = include_str!("templates/channel.json");
    tokio::fs::write(&channel_file, &channel_template).await?;

    new_channels.push(ChannelConfig::scaffold(number));
    new_channels.sort_by_key(|c| c.number.clone());

    let new_lineup = LineupConfig {
        channels: new_channels,
        ..lineup_config
    };

    let lineup_json =
        serde_json::to_string_pretty(&new_lineup).map_err(LineupError::ScaffoldSerializeError)?;

    tokio::fs::write(lineup_path, &lineup_json).await?;

    Ok(())
}
