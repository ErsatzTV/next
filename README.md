> [!CAUTION]
> **VERY EARLY STAGE:** This project is a complete rewrite of ErsatzTV in Rust. It is currently in active development, experimental, and *not yet ready* for production use. Expect breaking changes, missing features, and bugs.

# ErsatzTV

ErsatzTV is a modular, self-hosted IPTV server that transcodes and streams your media as live TV channels.

## Background

This rewrite focuses on a "one thing well" philosophy: **reliable transcoding and streaming**.

> [!IMPORTANT]
> Library and metadata management, scheduling and playout creation **are not in scope for this project**.

Unlike [the legacy version](https://github.com/ErsatzTV/legacy), this version is decoupled from library management and scheduling. It consumes **playouts** (JSON documents describing what to play and when) and handles the heavy lifting of keeping a stream alive and consistent, regardless of source media variations.

## Documentation

Platform-specific quickstart guides and reference documentation live at **<https://ersatztv.org/next-docs/>**.

## Contents

This project contains the following crates:

- [ffpipeline](crates/ffpipeline): transcoding and normalization logic
- [ersatztv-playout](crates/ersatztv-playout): Rust models for the playout JSON schema
- [ersatztv-channel](crates/ersatztv-channel): generates a normalized IPTV stream for a single channel from playout JSON
- [ersatztv](crates/ersatztv): serves IPTV over HTTP (M3U, M3U8, etc.) and manages channel processes
- [ersatztv-playout-generator](crates/ersatztv-playout-generator): generates playout JSON from a folder of video files. *Provided for demonstration and reference purposes; scheduling is not in scope and feature requests will not be accepted.*

Finally, there are configuration examples under [examples](examples):

- [playout.json](examples/playout/playout.json): an example playout JSON file, demonstrating some of the possible fields.
- [channel.json](examples/channel.json): an example channel configuration, linking a channel to its playout JSON files, and describing how to normalize the content.
- [lineup.json](examples/lineup.json): an example lineup configuration, linking to all channels, and describing where to write the normalized content and how to serve it over HTTP.

## Getting Started

### Prerequisites

- `ffmpeg` and `ffprobe` must be in your `PATH` (or referenced by absolute path in `channel.json`).

### Install

Grab a pre-built binary from the [develop release](https://github.com/ErsatzTV/next/releases/tag/develop), or build from source with `cargo build --release --workspace`.

For platform-specific walkthroughs, see the [docs site](https://ersatztv.org/next-docs/).

### Quick Start

1. **Scaffold a lineup with one channel:**
   ```bash
   ersatztv add-lineup config/lineup.json --channels 1
   ```
   This creates `config/lineup.json`, `config/channels/1/channel.json`, and `config/channels/1/playout/`.

2. **Generate a test playout** from a folder of video files:
   ```bash
   ersatztv-playout-generator --lineup config/lineup.json --channel 1 --content-folder /path/to/videos
   ```

3. **Run the server:**
   ```bash
   ersatztv config/lineup.json
   ```

4. **Watch** at `http://localhost:8409/channel/1.m3u8` in VLC, mpv, or any HLS player. For a no-install check, open the [hls.js demo](https://hlsjs.video-dev.org/demo/?src=http%3A%2F%2Flocalhost%3A8409%2Fchannel%2F1.m3u8).

## Contributing

We welcome early feedback and contributions!

- **Matrix:** [#ersatztv-dev:matrix.org](https://matrix.to/#/#ersatztv-dev:matrix.org)
- **Discord:** [#developer-chat](https://discord.ersatztv.org)

Early feedback on the **playout schema** and architecture is especially valuable at this stage.

## License

ErsatzTV is licensed under the [MIT License](LICENSE).
