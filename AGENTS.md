# AGENTS.md

This file provides guidance to AI agents, e.g. Claude Code (claude.ai/code) when working with code in this repository.

## What is this?

ErsatzTV (next) is a Rust rewrite of ErsatzTV — a self-hosted IPTV server that transcodes and streams media as live TV
channels over HTTP/HLS. It intentionally excludes library management and scheduling; it consumes pre-defined playout
JSON files and handles transcoding/streaming.

Any scheduler that can write the playout and channel JSON can drive this project; the legacy C# ErsatzTV project
(github.com/ErsatzTV/legacy) is the primary integrator to date and mirrors several data models here. Other
integrators exist, so the JSON schemas in `schema/` are the public contract — not the Rust types. See "Cross-repo
coupling" below.

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

# Probe hardware acceleration capabilities directly (one subcommand per accel)
cargo run --bin probe_capabilities -- <amf|cuda|qsv|rkmpp|vaapi|video-toolbox|vulkan|opencl>

# Generate test playout from video files (explicit output folder)
cargo run --bin ersatztv-playout-generator -- --content-folder <dir> --output-folder <dir>

# Generate test playout for a channel in a lineup (resolves the playout folder from channel.json)
cargo run --bin ersatztv-playout-generator -- --content-folder <dir> --lineup <path/to/lineup.json> --channel <N>

# Regenerate JSON schemas from Rust types (see "Generated vs hand-maintained files")
cargo run --bin gen-channel-config-schema > schema/channel_config.json
cargo run --bin gen-lineup-config-schema > schema/lineup_config.json

# Lint (CI also sets RUSTFLAGS=-Dwarnings, so warnings fail the build)
RUSTFLAGS=-Dwarnings cargo clippy --locked --workspace --all-features --all-targets -- -D clippy::all

# Format (requires nightly)
cargo +nightly fmt --all

# Format check
cargo +nightly fmt --all -- --check

# There are 2 styles of tests in the repository currently, unit and lightweight integration
# Lightweight integration tests are disabled by default because they require local ffmpeg
# binaries.

# Running the tests:
cargo test

# Run all integration tests explicitly (always --test-threads 1; see caveats below)
cargo test --package ffpipeline -- --ignored --test-threads 1

# Run just software or one hardware suite
cargo test --package ffpipeline --test software -- --ignored --test-threads 1
cargo test --package ffpipeline --test qsv -- --ignored --test-threads 1   # also: amf, cuda, rkmpp, vaapi, videotoolbox

# Point integration tests at a specific ffmpeg build
ETV_TEST_FFMPEG=/path/to/ffmpeg ETV_TEST_FFPROBE=/path/to/ffprobe cargo test --package ffpipeline -- --ignored --test-threads 1

# VAAPI suite: override the render node / driver (defaults: /dev/dri/renderD128, auto-detect iHD/i965/radeonsi)
ETV_TEST_VAAPI_DEVICE=/dev/dri/renderD129 ETV_TEST_VAAPI_DRIVER=iHD cargo test --package ffpipeline --test vaapi -- --ignored --test-threads 1

# Run integration tests against the exact ffmpeg build CI/docker users get (test-runtime target in docker/Dockerfile).
# Args: [suite ...] [filter ...] [-- libtest-args]; pass the GPU through as you would for the app image.
docker build --target test-runtime -t next-test -f docker/Dockerfile .
docker run --rm next-test software                # one suite
docker run --rm --device /dev/dri next-test qsv tonemap  # one suite, tests matching "tonemap"
```

CI runs unit tests on every platform and a single software integration test (`codec_copy`) in the docker test image.
It does **not** run the integration suites; those are the contributor's responsibility.

### Integration test caveats

- Run integration tests with `--test-threads 1`. Hardware suites are unstable when tests share a GPU concurrently,
  and each test spawns a real ffmpeg process, so parallel runs also contend for encoders and skew timing-sensitive
  assertions. A failure seen only under parallel execution is not a bug report.
- Hardware suites (`tests/{amf,cuda,qsv,rkmpp,vaapi,videotoolbox}.rs`) fail when the ffmpeg build lacks the hwaccel,
  the device cannot be opened, or capability probing reports nothing, so only run the suites for hardware you have.
  Each test also asserts that the pipeline's decode/encode choice matches what the accel's capability probe reports
  (`assert_accel_usage` in `tests/common/mod.rs`), so a device that genuinely lacks e.g. a HEVC encoder passes on
  the software path while a probe or pipeline regression that silently falls back to software fails.
- The software suite requires `ffmpeg`/`ffprobe` on `PATH` (or `ETV_TEST_FFMPEG`/`ETV_TEST_FFPROBE`) and assumes the
  ErsatzTV ffmpeg build (github.com/ErsatzTV/ErsatzTV-ffmpeg); stock ffmpeg lacks some patched filters and will fail
  the tests that need them. `ETV_TEST_DISABLED_FILTERS=<comma list>` hides filters from `FfmpegInfo` so the pipeline
  builder takes its fallback path; it exists to test fallbacks (e.g. `pad_opencl` when `pad_vaapi` is hidden), not to
  make a stock-ffmpeg run pass.
- Test inputs live in `crates/ffpipeline/tests/fixtures/` and are referenced by name in `#[values(...)]` lists in each
  suite. A new fixture is not tested until it is added to those lists.

## Architecture

### Process Model

The server (`ersatztv`) spawns a separate `ersatztv-channel` subprocess per active channel. Processes communicate via
file-based signaling (`.ready` and `.heartbeat` files) — no IPC. The main server monitors these files with tokio watch
channels.

### Crate Structure

- **`ersatztv`** — Axum HTTP server. Serves M3U/M3U8 playlists, manages channel process lifecycle via
  `ChannelSession::spawn()`. Routes: `/channels.m3u`, `/channel/{N}.m3u8`, `/session/{channel}/{file}`. Owns the
  `lineup.json` model and the `templates/channel.json` used by `add-lineup`/`add-channel` scaffolding.
- **`ersatztv-channel`** — Per-channel worker. Reads playout JSON, builds FFmpeg pipelines, generates HLS segments. Has
  a 4-state machine (`SeekAndWorkAhead` → `ZeroAndWorkAhead` → `SeekAndRealtime` → `ZeroAndRealtime`) for buffering
  strategy. Owns the `channel.json` model (`config.rs`), the fallback renderer (`fallback.rs`), the graphics-canvas
  local proxy (`local_proxy.rs`), and `probe_hint_to_result()` which turns playout probe hints into `ProbeResult`s.
- **`ffpipeline`** — FFmpeg pipeline builder. Probes source media (`probe.rs`), detects ffmpeg build features
  (`ffmpeg_info.rs`), selects hardware acceleration, constructs filter chains, generates ffmpeg command-line args.
  Key trait: `HwAccel` (`src/accel/`) with implementations for AMF, CUDA, QSV, RKMPP, VAAPI, VideoToolbox, Vulkan.
  Capability detection per accel lives in `src/capabilities/<accel>/` (CUDA's is `nvidia/`) with a `stub.rs` for
  unsupported platforms.
- **`ersatztv-playout`** — Playout JSON data models (serde), including `ProbeHint`. Schema at `schema/playout.json`
  is hand-maintained — keep it in sync when editing the Rust types.
- **`ersatztv-core`** — Shared utilities: heartbeat/ready file management, timing constants.
- **`ersatztv-playout-generator`** — Dev tool for generating playout JSON from video folders or syncing from legacy DB.
  Not a supported feature; exists so `ProbeHint` generation has a reference implementation and for local testing.
- **`libamf-sys`, `libcl-sys`, `libd3d11-sys`, `libmpp-sys`, `libnvidia-sys`, `libva-sys`, `libvpl-sys`,
  `libvt-sys`, `libvulkan-sys`** — FFI bindings for hardware acceleration capability detection. Platform-specific with
  stub fallbacks.

### Configuration Tiers

1. **`lineup.json`** — Server bind address, port, output folder, list of channels (each referencing a channel config)
2. **`channel.json`** — Playout folder, FFmpeg paths, normalization settings (video codec/resolution/bitrate, audio codec/bitrate, hardware acceleration)
3. **Playout JSON files** — Named `{start}_{finish}.json` with ISO 8601 timestamps. Loaded on-demand based on current time.

### Key Design Decisions

- Hardware acceleration is auto-detected at runtime via FFI capability probing, with graceful fallback
- HLS segments are 4 seconds; keyframe interval is 2 seconds
- The server is stateless — all state lives in config files and the filesystem (HLS segments, signal files)
- Playout files can be updated on disk without restarting the server
- Playout items may carry a `probe_hint` so the channel can skip `ffprobe` before playback. Any field that
  `ProbeResult` needs must therefore also exist in the hint, or hinted items silently behave differently from probed
  ones.

## Generated vs hand-maintained files

| File | Source of truth | How to update |
|---|---|---|
| `schema/channel_config.json` | `crates/ersatztv-channel/src/config.rs` (schemars) | `cargo run --bin gen-channel-config-schema > schema/channel_config.json` |
| `schema/lineup_config.json` | `crates/ersatztv/src/config.rs` (schemars) | `cargo run --bin gen-lineup-config-schema > schema/lineup_config.json` |
| `schema/playout.json` | hand-maintained | edit by hand alongside `crates/ersatztv-playout/src/playout.rs` |
| `crates/ersatztv/src/templates/channel.json` | hand-maintained | update when a channel config field is added/renamed/removed |
| `examples/channel.json`, `examples/lineup.json`, `examples/playout/playout.json` | hand-maintained | update when the corresponding model changes; nothing loads these in tests, so check them by hand |

Never hand-edit a generated schema. If the generated output differs from the committed file for reasons unrelated to
your change, regenerate anyway and note it in the PR.

## Change checklists

These are the file sets that historically get touched together. Missing one usually compiles and passes tests, then
fails in production. Work through the whole list for your change type.

### Adding or changing a probe field (e.g. a new color/HDR/DV property)

1. `crates/ffpipeline/src/probe.rs` — add to `ProbeResultVideoStream` / `ProbeResultAudioStream` /
   `ProbeResultColorParams` and parse it from `ffprobe` output.
2. `crates/ersatztv-playout/src/playout.rs` — add the matching field to `VideoHint` / `AudioHint` / `SubtitleHint`.
   Use `Option<T>` + `#[serde(skip_serializing_if = "Option::is_none")]` so older playout files still load.
3. `schema/playout.json` — add the property (hand-maintained), and bump `SUPPORTED_SCHEMA.compatible` in
   `playout.rs` (see "Playout schema versioning").
4. `crates/ersatztv-channel/src/channel_session.rs` — map the hint field in `probe_hint_to_result()`. If you skip
   this, hinted items lose the field and the pipeline takes a different path than for probed items.
5. `crates/ersatztv-playout-generator/src/generate.rs` — populate the hint from `ProbeResult` so generated playouts
   serve as the reference for what integrators should emit.
6. Consumers: wherever the field changes pipeline decisions (`pipeline.rs`, `filter_chain.rs`, `src/accel/*.rs`).
7. `crates/ffpipeline/tests/common/mod.rs` — extend the output assertions if the field affects output
   (e.g. `assert_sdr_output` verifies tonemapping actually happened rather than silently no-op'ing).
8. Cross-repo: the legacy converter must emit the new field (see "Cross-repo coupling").

### Adding a channel config option (`channel.json`)

1. `crates/ersatztv-channel/src/config.rs` — add the field with a `///` doc comment (it becomes the schema
   description) and a serde default so existing configs load.
2. Regenerate `schema/channel_config.json` (see above). Commit the regenerated file.
3. `crates/ersatztv/src/templates/channel.json` and `examples/channel.json` — add the field if it is something a
   user would reasonably set.
4. Thread the value from config into `ffpipeline` (`OutputSettings` / `VideoFilterOptions` / `InputSettings`) and
   into `channel_session.rs`.
5. Cross-repo: legacy's generated `ChannelConfig.cs` must be regenerated (see "Cross-repo coupling").

### Adding a lineup config option (`lineup.json`)

Same shape as channel config: edit the lineup model in `crates/ersatztv`, regenerate `schema/lineup_config.json`,
update `examples/lineup.json`.

### Adding a playout item source kind, graphics layer kind, or other playout model change

1. `crates/ersatztv-playout/src/playout.rs` — the enum/struct change.
2. `schema/playout.json` — mirror it by hand; bump `SUPPORTED_SCHEMA`.
3. `crates/ersatztv-channel/src/channel_session.rs` — handle the new variant (`match` arms are exhaustive; the
   compiler will find most of these).
4. `examples/playout/playout.json` if it is user-facing.
5. Cross-repo: legacy `Core/Next/Playout.cs` and `PlayoutItemConverter.cs`.

### Adding a hardware capability to an existing accel (e.g. "this GPU can tonemap")

1. `crates/lib<accel>-sys/src/lib.rs` — FFI surface, if the driver API is needed.
2. `crates/ffpipeline/src/capabilities/<accel>/{mod.rs, <backend>.rs, stub.rs}` — detect it; **the stub must
   report the capability as absent** so non-native platforms still compile and fall back.
3. `crates/ffpipeline/src/accel/<accel>.rs` — use the capability when building the pipeline.
4. `crates/ffpipeline/src/bin/probe_capabilities.rs` — print it, so users can report what their hardware detects.
5. `crates/ffpipeline/tests/<accel>.rs` — a test case that exercises the new path; note in the PR whether you ran it
   on real hardware. If the capability changes decoder/encoder selection, `assert_accel_usage` in
   `tests/common/mod.rs` must still agree with the accel's `can_decode`/`can_encode`.

### Adding a new hardware accelerator

Everything in the previous checklist, plus:

1. New `crates/lib<accel>-sys` crate; add to workspace `Cargo.toml` and `ffpipeline/Cargo.toml`.
2. `crates/ffpipeline/src/capabilities/mod.rs` and `src/accel/mod.rs` — register the module.
3. `crates/ffpipeline/src/hw_accel.rs` — `HardwareAccel` enum variant.
4. `crates/ffpipeline/src/ffmpeg_info.rs` — `KnownHardwareAccel` / `KnownDecoders` / `KnownVideoFilter` entries for
   the ffmpeg hwaccel, decoder and filter names to probe for.
5. `crates/ffpipeline/src/error.rs` — accel-specific error variants.
6. `crates/ersatztv-channel/src/config.rs` — the config-side `HardwareAccel` enum (separate from ffpipeline's) and its
   `to_pipeline()` arm; regenerate `schema/channel_config.json`.
7. New `crates/ffpipeline/tests/<accel>.rs` following the existing suites (gate on capability probing, `#[ignore]`).
8. This file — add the accel to the crate list above and the test command list.

### Adding codec/format support to an accel (e.g. "AMF can now hw-decode mpeg2")

1. `crates/ffpipeline/src/ffmpeg_info.rs` — decoder/filter names to detect in the ffmpeg build.
2. `crates/ffpipeline/src/accel/<accel>.rs` — decoder selection and any pix_fmt / hwupload changes.
3. `crates/ffpipeline/src/capabilities/<accel>/` if it depends on hardware generation.
4. `crates/ffpipeline/tests/fixtures/` — a fixture in that codec if none exists, added to the relevant `#[values]`
   lists in `tests/<accel>.rs` and `tests/software.rs`.

### Playout schema versioning

`crates/ersatztv-playout/src/playout.rs` declares `SUPPORTED_SCHEMA { breaking, compatible }` and playout files
carry `"version": "https://ersatztv.org/playout/version/0.<breaking>.<compatible>"`. Loading fails if the file's
`breaking` differs or its `compatible` is newer than what this build supports.

Rule: **any change to `schema/playout.json` bumps `compatible`**, including optional additive fields. Renames,
removals, or semantic changes to existing fields bump `breaking` and reset `compatible` to 0. Legacy emits the
version it was built against, so a bump here requires a matching change in every integrator (legacy included) before
they can be deployed against this build.

## Cross-repo coupling

Changes to the JSON contract affect every integrator. The legacy repo (github.com/ErsatzTV/legacy) is the one the
maintainer keeps in lockstep and is listed here as the concrete example; the docs site (github.com/ErsatzTV/ErsatzTV.org)
is how other integrators learn about the change. **Contributors are not expected to make those changes** — the
maintainer handles the legacy side. Your job is to call it out explicitly in the PR description so it isn't missed:

| Change in next | Legacy file(s) affected |
|---|---|
| `ProbeHint` / `VideoHint` / `AudioHint` / `SubtitleHint` field | `ErsatzTV.Core/Next/Playout.cs`, `ErsatzTV.Infrastructure/Scheduling/PlayoutItemConverter.cs`, possibly `ErsatzTV.Core/Domain/MediaItem/MediaStream.cs` + a migration if the value isn't already stored |
| Any other `playout.rs` model change | `ErsatzTV.Core/Next/Playout.cs`, `PlayoutItemConverter.cs` |
| `SUPPORTED_SCHEMA` bump | the hardcoded version string in legacy's `PrepareTroubleshootingPlaybackHandler.cs` and `PlayoutItemConverterTests.cs` (and whatever other integrators write) |
| `channel.json` field | `ErsatzTV.Core/Next/Config/ChannelConfig.cs` (generated from `schema/channel_config.json` via quicktype; never hand-edited) |
| User-visible config or behavior | `ErsatzTV.org/docs-next/` |

## Pull request conventions

- PR titles are conventional commits (`feat:`, `fix:`, `refactor:`, `build:`, `docs:`, `test:`, `ci:`, `chore:`,
  optionally scoped like `feat(pipeline):`); PRs are squash-merged and the title becomes the commit message.
- Before opening: `cargo +nightly fmt --all`, the clippy command above with `RUSTFLAGS=-Dwarnings`, `cargo test`,
  and `cargo test --package ffpipeline --test software -- --ignored --test-threads 1`. CI builds on Windows, Linux
  (glibc + musl, x64 + arm + arm64) and macOS (x64 + arm64), so platform-specific code needs a stub path for the
  others.
- In the PR description, state: which integration suites you ran and on what hardware/ffmpeg build, and any
  cross-repo follow-ups from the table above.
- Keep comments minimal; explain non-obvious *why*, not *what*.
