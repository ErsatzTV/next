> [!WARNING]
> **PRE-1.0:** This project is a complete rewrite of the ErsatzTV streaming engine in Rust. It ships today as an optional per-channel engine in [ErsatzTV (legacy)](https://github.com/ErsatzTV/legacy), and can also run standalone. Configuration and playout formats are versioned but may still change in breaking ways before 1.0; expect missing features and bugs.

# ErsatzTV

ErsatzTV is a modular, self-hosted IPTV server that transcodes and streams your media as live TV channels.

## Background

This rewrite focuses on a "one thing well" philosophy: **reliable transcoding and streaming**.

> [!IMPORTANT]
> Library and metadata management, scheduling and playout creation **are not in scope for this project**.

Unlike [the legacy version](https://github.com/ErsatzTV/legacy), this version is decoupled from library management and scheduling. It consumes **playouts** (JSON documents describing what to play and when) and handles the heavy lifting of keeping a stream alive and consistent, regardless of source media variations.

### Ways to use it

- **With ErsatzTV (legacy):** set a channel's **Streaming Engine** to **Next**. Legacy keeps handling libraries and scheduling, and writes the playout and channel configuration that this engine consumes. No separate install is needed.
- **Standalone:** bring your own scheduler (or hand-written playouts) that writes JSON matching the [schemas](schema), and run the `ersatztv` server described below.

## Features

- **Hardware acceleration:** AMF, CUDA, QSV, RKMPP, VAAPI, VideoToolbox and Vulkan. Capabilities are probed at runtime, and anything the hardware can't do falls back to software.
- **Normalization:** every item is transcoded to the channel's configured format, resolution and audio layout, including HDR10 and Dolby Vision tonemapping, deinterlacing, and anamorphic and rotated sources.
- **Stream copy:** channels can copy video and/or audio instead of transcoding when the source already matches.
- **Overlays:** watermarks, graphics layers and burned-in subtitles.
- **Sources:** local files, HTTP and RTSP streams, commands that write MPEG-TS to stdout, and dynamic items that are resolved over HTTP at playback time.
- **Gap filling:** schedule gaps, missing playouts and items that fail to transcode are replaced with a fallback stream instead of dropping the channel.
- **IPTV output:** HLS per channel, an M3U channel list, and XMLTV guide data.

## Documentation

Quickstart guides and reference documentation live at **<https://ersatztv.org/next-docs/>**.

## Contents

This project contains the following crates:

- [ffpipeline](crates/ffpipeline): transcoding and normalization logic
- [ersatztv-playout](crates/ersatztv-playout): Rust models for the playout JSON schema
- [ersatztv-channel](crates/ersatztv-channel): generates a normalized IPTV stream for a single channel from playout JSON
- [ersatztv](crates/ersatztv): serves IPTV over HTTP (M3U, M3U8, XMLTV) and manages channel processes
- [ersatztv-core](crates/ersatztv-core): shared utilities, including config merging and schema versioning
- [ersatztv-playout-generator](crates/ersatztv-playout-generator): generates playout JSON from a folder of video files. *Provided for demonstration and reference purposes; scheduling is not in scope and feature requests will not be accepted.*
- `lib*-sys` (e.g. [libva-sys](crates/libva-sys), [libvpl-sys](crates/libvpl-sys)): FFI bindings used to probe hardware acceleration capabilities

The JSON schemas under [schema](schema) are the public contract for integrators: [playout.json](schema/playout.json), [channel_config.json](schema/channel_config.json) and [lineup_config.json](schema/lineup_config.json). Each document carries a `version`; files with an incompatible version are rejected with an error rather than misread.

Finally, there are configuration examples under [examples](examples):

- [playout.json](examples/playout/playout.json): an example playout JSON file, demonstrating some of the possible fields.
- [channel.json](examples/channel.json): an example channel configuration, linking a channel to its playout JSON files, and describing how to normalize the content.
- [channel_copy.json](examples/channel_copy.json): an example channel configuration that copies video and audio when the source format allows it.
- [lineup.json](examples/lineup.json): an example lineup configuration, linking to all channels, and describing where to write the normalized content and how to serve it over HTTP.

## Getting Started

### Prerequisites

- **ErsatzTV's ffmpeg build.** Use `ffmpeg` and `ffprobe` from [ErsatzTV-ffmpeg](https://github.com/ErsatzTV/ErsatzTV-ffmpeg/releases/latest), either in your `PATH` or referenced by absolute path in `channel.json`. It carries patches and fixes that this project relies on; stock or distro ffmpeg builds are not supported. The Docker image already includes it.

### Install

- **Docker:** `ersatztv/next:develop` (also `ghcr.io/ersatztv/next:develop`) for `linux/amd64` and `linux/arm64`.
- **Binaries:** download a build for Windows, Linux (x64, x64 musl, arm64) or macOS (x64, arm64) from the [develop release](https://github.com/ErsatzTV/next/releases/tag/develop).
- **Source:** `cargo build --release --workspace`.

### Quick Start

1. **Scaffold a lineup with one channel:**
   ```bash
   ersatztv add-lineup config/lineup.json --channels 1
   ```
   This creates `config/lineup.json`, `config/hls/`, `config/channels/1/channel.json`, and `config/channels/1/playout/`.

2. **Generate a test playout** from a folder of video files:
   ```bash
   ersatztv-playout-generator --lineup config/lineup.json --channel 1 --content-folder /path/to/videos
   ```

3. **Run the server:**
   ```bash
   ersatztv config/lineup.json
   ```

4. **Watch** at `http://localhost:8409/channel/1.m3u8` in VLC, mpv, or any HLS player. For a no-install check, open the [hls.js demo](https://hlsjs.video-dev.org/demo/?src=http%3A%2F%2Flocalhost%3A8409%2Fchannel%2F1.m3u8).

   IPTV clients can use the channel list at `http://localhost:8409/channels.m3u` and guide data at `http://localhost:8409/xmltv.xml`.

### Quick Start (Docker)

The image runs `ersatztv /config/lineup.json`. Media paths in your playouts must be valid inside the container, so mount your media at the same path you use in the playouts:

```bash
# scaffold
docker run --rm -v ./config:/config ersatztv/next:develop add-lineup /config/lineup.json --channels 1

# generate a test playout
docker run --rm -v ./config:/config -v /path/to/videos:/path/to/videos \
  --entrypoint /app/ersatztv-playout-generator ersatztv/next:develop \
  --lineup /config/lineup.json --channel 1 --content-folder /path/to/videos

# run the server
docker run -d -p 8409:8409 -v ./config:/config -v /path/to/videos:/path/to/videos ersatztv/next:develop
```

For hardware acceleration, pass the device through: `--device /dev/dri` for VAAPI/QSV, or `--gpus all` for NVIDIA.

## Reporting Issues

When a stream fails, the most useful report includes:

- the output of `ersatztv-channel debug <path/to/channel.json>`, which logs the merged channel config, the ffmpeg build features and the hardware capabilities this engine detected;
- an error dossier: set `ffmpeg.reports_folder` in `channel.json`, and each failed transcode writes a folder containing the ffmpeg report and stderr, the generated pipeline, the playout item, media info and the channel config.

## Contributing

We welcome early feedback and contributions!

- **Matrix:** [#ersatztv-dev:matrix.org](https://matrix.to/#/#ersatztv-dev:matrix.org)
- **Discord:** [#developer-chat](https://discord.ersatztv.org)

Early feedback on the **playout schema** and architecture is especially valuable at this stage.

## License

ErsatzTV is licensed under the [MIT License](LICENSE).
