> [!CAUTION]
> **VERY EARLY STAGE:** This project is a complete rewrite of ErsatzTV in Rust. It is currently in active development, experimental, and *not yet ready* for production use. Expect breaking changes, missing features, and bugs.

# ErsatzTV

ErsatzTV is a modular, self-hosted IPTV server that transcodes and streams your media as live TV channels.

## Background

This rewrite focuses on a "one thing well" philosophy: **reliable transcoding and streaming**.

> [!IMPORTANT]
> Library and metadata management, scheduling and playout creation **are not in scope for this project**.

Unlike [the legacy version](https://github.com/ErsatzTV/ErsatzTV-legacy), this version is decoupled from library management and scheduling. It consumes **playouts** (JSON documents describing what to play and when) and handles the heavy lifting of keeping a stream alive and consistent, regardless of source media variations.

## Contents

This project contains the following crates:

- [ffpipeline](crates/ffpipeline): transcoding and normalization logic
- [ersatztv-playout](crates/ersatztv-playout): Rust models for the playout JSON schema
- [ersatztv-channel](crates/ersatztv-channel): generates a normalized IPTV stream for a single channel from playout JSON
- [ersatztv](crates/ersatztv): serves IPTV over HTTP (M3U, M3U8, etc.) and manages channel processes
- [ersatztv-playout-generator](crates/ersatztv-playout-generator): generates playout JSON from a folder of video files. *This is provided for development and testing only.*

Finally, there are configuration examples under [examples](examples):

- [playout.json](examples/playout/playout.json): an example playout JSON file, demonstrating some of the possible fields.
- [channel.json](examples/channel.json): an example channel configuration, linking a channel to its playout JSON files, and describing how to normalize the content.
- [lineup.json](examples/lineup.json): an example lineup configuration, linking to all channels, and describing where to write the normalized content and how to serve it over HTTP.

## Getting Started

### Prerequisites

- `ffmpeg` and `ffprobe` must be in your `PATH`.
- Rust toolchain (if building from source).

### Quick Start

1. **Clone the repo:**
   ```bash
   git clone https://github.com/ErsatzTV/next.git
   cd next
   ```

2. **Generate a test playout:**
   Point this at a folder with some video files.
   ```bash
   cargo run --bin ersatztv-playout-generator -- --content-folder "/path/to/videos" --output-folder "config/channels/1/playout"
   ```

3. **Configure your Channel:**
   Copy [channel.json](examples/channel.json) to `config/channels/1/channel.json`. Update the playout folder to point to the folder created in step 2.

4. **Configure your Lineup:**
   Copy [lineup.json](examples/lineup.json) to `config/lineup.json`. Update the channel path to point to your `config/channels/1/channel.json`.

5. **Run the server:**
   ```bash
   cargo run --bin ersatztv -- config/lineup.json
   ```

6. **Watch:**
   Open `http://localhost:8409/channel/1.m3u8` in VLC, mpv, or any HLS player.

## Contributing

We welcome early feedback and contributions!

- **Matrix:** [#ersatztv-dev:matrix.org](https://matrix.to/#/#ersatztv-dev:matrix.org)
- **Discord:** [#developer-chat](https://discord.ersatztv.org)

Early feedback on the **playout schema** and architecture is especially valuable at this stage.

## License

ErsatzTV is licensed under the [MIT License](LICENSE).
