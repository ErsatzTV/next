# AGENTS.md

This file provides guidance to AI agents, e.g. Claude Code (claude.ai/code) when working with code in this repository.

## What is this?

ErsatzTV (next) is a Rust rewrite of ErsatzTV — a self-hosted IPTV server that transcodes and streams media as live TV
channels over HTTP/HLS. It intentionally excludes library management and scheduling; it consumes pre-defined playout
JSON files and handles transcoding/streaming.

## Build & Development Commands

```bash
# Build
cargo build --workspace --all-features

# Run the IPTV server
cargo run --bin ersatztv -- <path/to/lineup.json>

# Scaffold a new lineup with N channels (creates lineup.json, hls/, channels/<N>/{channel.json,playout/})
cargo run --bin ersatztv -- add-lineup <path/to/lineup.json> --channels <N>

# Add a channel to an existing lineup
cargo run --bin ersatztv -- add-channel <path/to/lineup.json> --number <X>

# Run a single channel worker (usually spawned by the server)
cargo run --bin ersatztv-channel -- run <path/to/channel.json> --output-folder <dir> --number <N>

# Debug channel config and FFmpeg capabilities
cargo run --bin ersatztv-channel -- debug <path/to/channel.json>

# Generate test playout from video files (explicit output folder)
cargo run --bin ersatztv-playout-generator -- --content-folder <dir> --output-folder <dir>

# Generate test playout for a channel in a lineup (resolves the playout folder from channel.json)
cargo run --bin ersatztv-playout-generator -- --content-folder <dir> --lineup <path/to/lineup.json> --channel <N>

# Lint
cargo clippy --locked --workspace --all-features --all-targets -- -D clippy::all

# Format (requires nightly)
cargo +nightly fmt --all

# Format check
cargo +nightly fmt --all -- --check

# There are 2 styles of tests in the repository currently, unit and lightweight integration
# Lightweight integration tests are disabled by default because they require local ffmpeg
# binaries.

# Running the tests:
cargo test

# Run all integration tests explicitly
cargo test --package ffpipeline -- --ignored

# Run just software or hardware tests
cargo test --package ffpipeline --test software -- --ignored
cargo test --package ffpipeline --test videotoolbox -- --ignored
```

## Architecture

### Process Model

The server (`ersatztv`) spawns a separate `ersatztv-channel` subprocess per active channel. Processes communicate via
file-based signaling (`.ready` and `.heartbeat` files) — no IPC. The main server monitors these files with tokio watch
channels.

### Crate Structure

- **`ersatztv`** — Axum HTTP server. Serves M3U/M3U8 playlists, manages channel process lifecycle via
  `ChannelSession::spawn()`. Routes: `/channels.m3u`, `/channel/{N}.m3u8`, `/session/{channel}/{file}`.
- **`ersatztv-channel`** — Per-channel worker. Reads playout JSON, builds FFmpeg pipelines, generates HLS segments. Has
  a 4-state machine (`SeekAndWorkAhead` → `ZeroAndWorkAhead` → `SeekAndRealtime` → `ZeroAndRealtime`) for buffering
  strategy.
- **`ffpipeline`** — FFmpeg pipeline builder. Probes source media, selects hardware acceleration, constructs filter
  chains, generates ffmpeg command-line args. Key trait: `HwAccel` with implementations for CUDA, QSV, VAAPI,
  VideoToolbox.
- **`ersatztv-playout`** — Playout JSON data models (serde). Schema at `schema/playout.json` is hand-maintained - keep it in sync when editing the Rust types.
- **`ersatztv-core`** — Shared utilities: heartbeat/ready file management, timing constants.
- **`ersatztv-playout-generator`** — Dev tool for generating playout JSON from video folders or syncing from legacy DB.
- **`libnvidia-sys`, `libva-sys`, `libvpl-sys`** — FFI bindings for hardware acceleration capability detection.
  Platform-specific with stub fallbacks.

### Configuration Tiers

1. **`lineup.json`** — Server bind address, port, output folder, list of channels (each referencing a channel config)
2. **`channel.json`** — Playout folder, FFmpeg paths, normalization settings (video codec/resolution/bitrate, audio codec/bitrate, hardware acceleration)
3. **Playout JSON files** — Named `{start}_{finish}.json` with ISO 8601 timestamps. Loaded on-demand based on current time.

### Key Design Decisions

- Hardware acceleration is auto-detected at runtime via FFI capability probing, with graceful fallback
- HLS segments are 4 seconds; keyframe interval is 2 seconds
- The server is stateless — all state lives in config files and the filesystem (HLS segments, signal files)
- Playout files can be updated on disk without restarting the server
