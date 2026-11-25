# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0, the minor version marks breaking changes
(`0.B.C`, the same rule as the [config versions](https://ersatztv.org/next-docs/configuration/versions)).

## [Unreleased]

## [0.2.8] - 2026-11-25
### Fixed
- Video copy: fix frozen video or early fallback after seeking into MKV and MP4 sources
  - This happened when a seek dropped the planned start keyframe (e.g. 23.976 fps B-pyramid or open-GOP HEVC)
- Docker VAAPI: fix radeonsi h264 encode hang
- Fix audio drifting out of sync over time on MPEG-TS clients
  - This happened when audio overlapped at item boundaries (e.g. Plex Live TV, VLC)

## [0.2.7] - 2026-11-20
### Fixed
- VAAPI: fix ffmpeg crashes (and stopped channels) when less than one frame of an item is left
- CUDA: reduce channel startup latency
- Fix slow channel startup with libplacebo tone mapping
  - Compiled shaders are cached in `cache.folder`
- Docker: fix slow channel startup with hardware acceleration
  - Compiled GPU kernels and shaders are cached in `/config/cache/cuda` and `/config/cache/xdg`
- Troubleshooting (`run --troubleshoot`): fix missing dossier when an item fails before ffmpeg starts
  - Invalid items are saved to `playout_item.json` as written in the playout file

## [0.2.6] - 2026-11-16
### Added
- Add `session.idle_timeout_seconds` to `channel.json` (channel config schema `0.1.4`)
  - Seconds without a `.heartbeat` touch before the channel stops (default 90); `0` keeps the channel running without viewers
- Add `session.ready_segment_count` to `channel.json`
  - Segments the playlist must hold before `.ready` is written, from 1 to 10 (default 3)
- Add `cache.folder` and `cache.max_unused_days` to `channel.json` (channel config schema `0.1.5`)
  - Where rendered still-image graphics are kept (default an `ersatztv-next` folder in the system temp folder), and how many days an unused one is kept (default 30)

### Changed
- Render still-image graphics once into the cache folder instead of for every output frame
  - A large logo on a slow CPU no longer drops the stream below realtime
  - If rendering fails, the full-size image is used as before
- Write `.ready` after 3 segments instead of 4
  - ffmpeg, mpv and hls.js now start at the first segment, sooner and with the same buffer

### Fixed
- Fix JPEG (full-range) watermarks crushing blacks and raising contrast in the main video
  - This happened when the video is scaled or converted before a software overlay (e.g. any scaled source, or VideoToolbox)

## [0.2.5] - 2026-11-06
### Added
- Add `ersatztv-channel describe`, which prints JSON describing the binary (schema: `schema/describe.json`)
  - Version and commit
  - Playout and channel config schema versions it accepts
  - Output file names, segment and keyframe lengths
  - Video/audio formats, accels and subtitle modes the channel config accepts
  - Headers sent to dynamic source resolvers
- Add `ersatztv-channel describe --probe` to show what this machine can do
  - ffmpeg version and hwaccels, and for each accel on this platform whether it is available and which codecs and pixel formats it decodes and encodes in hardware
  - VAAPI is tried on every render node with each driver
  - Optional channel configs (partial is fine) mark the configured accel; `--ffmpeg <path>` chooses the ffmpeg build
- Send `x-etv-playout-schema` with dynamic source requests (playout schema `0.0.8`)
  - This is the newest playout schema version the channel accepts

### Changed
- Require ErsatzTV-ffmpeg `8.1.2-8`
- Play dynamic source responses with an invalid `probe_hint` without their hints, instead of showing the fallback
  - This matches playout file items

### Fixed
- Vulkan: fix hardware decode being chosen for formats the driver can't decode (e.g. H.264 10-bit on Intel GPUs on Windows)
- Vulkan: never select a CPU (llvmpipe) Vulkan device
- Name the field in channel config errors, e.g. `normalization: missing field audio` instead of `missing field audio`
- Fix QSV streams failing about 10 s in on pre-Tiger Lake Intel GPUs (regression in 0.2.4)
  - This caused errors like `Can't allocate a surface` or `Failed to query an output frame`
- Fix NVENC failing to start on very low frame rate sources (e.g. 1 fps slideshows)
  - This caused errors like `Gop Length should be greater than number of B frames + 1`
- VAAPI: fix `tonemap_opencl` and `pad_opencl` never being used on Intel GPUs when only the OpenCL runtime (`libOpenCL.so.1`) is installed
  - This affected the ErsatzTV-ffmpeg base docker image
- Fix still-image watermarks disappearing partway through an item, or (QSV) the item ending early
  - This happened when the source's video parameters change mid-stream (e.g. spliced or smart-cut files)
- Fix QSV, VAAPI and CUDA items failing when the source's color tags change mid-stream
- Fix playback of 4:2:2, 4:4:4 and 12-bit sources (e.g. ProRes, or HEVC/H.264 from cameras)
  - These are now decoded in software
- Fix one invalid playout item failing the whole playout file
  - This showed `failed to load playout JSON file` for every item
  - An invalid item now shows the fallback (and error card, if `fallback.show_error` is on) for its own time only, naming the item and the invalid field
  - An item with an invalid `probe_hint` plays without its hints, with a warning in the log
  - An item without a readable `id`, `start` and `finish` is skipped, and its time is treated as a gap

### Known issues
- VAAPI: channels with a watermark run far below realtime on Intel Meteor Lake and Arrow Lake with the free `intel-media-va-driver` package (Debian, Ubuntu)
  - Install `intel-media-va-driver-non-free`, or set `ffmpeg.disabled_filters` to `["overlay_vaapi"]` in the channel config
  - The docker image is not affected

## [0.2.4] - 2026-10-31
### Added
- VAAPI: add `libplacebo` tonemapping when a Vulkan device on the same GPU is available
  - `libplacebo` is preferred over `tonemap_opencl` and `tonemap_vaapi`, and shows Dolby Vision profile 5 with correct colors
  - Requires ErsatzTV-ffmpeg `8.1.2-6`
- Add channel config fields (channel config schema `0.1.3`):
  - `normalization.video.vaapi_low_power` (`auto` or `never`, default `auto`)
  - `fallback.background_image`, a still image shown instead of black when there is nothing to play or an item fails
    - Black is used for the rest of the session if the image fails to play
- Add channel numbers to `/channels.m3u` (`tvg-chno` and `channel-number`)
  - Clients that number channels from the playlist (e.g. Channels DVR) no longer assign their own numbers

### Changed
- VAAPI: encode HEVC with the low-power entrypoint when the GPU supports it
  - Set `normalization.video.vaapi_low_power` to `never` to keep the previous behavior
- AMF: decode MPEG-2 in software
  - This is as fast or faster, and avoids a hang on older (Polaris) GPUs

### Fixed
- Fix wrong aspect ratio (e.g. 16:9 squished to 4:3) when the probe hint reports square pixels at the output size
- Fix burning the wrong embedded text subtitle when a file has more than one subtitle stream
- Fix VAAPI stream failing to start on AMD when an OpenCL runtime (e.g. NVIDIA, rusticl) is installed
- Fix brief heavy blocking in VAAPI H.264 output with the `i965` driver after static scenes
- AMF:
  - Fix channels crashing on Windows before producing any output with some AMD drivers (e.g. RX 9000 series)
  - Fix washed-out colors in 10-bit output with some AMD drivers
  - Fix streams starting very slowly or failing to start in time
  - Fix streams hanging with loudness normalization when video is not scaled or converted on the GPU
- QSV:
  - Fix stream failing to start on Linux when the VPL runtime reports a decoder the VA driver has disabled (e.g. VC-1 and VP8 on Arc)
    - These formats are now decoded in software
  - Fix stream failing with `Can't allocate a surface` on pre-Tiger Lake Intel GPUs for files that store audio far behind video (e.g. some WMV)
  - Fix VC-1 streams failing or hanging on Windows with pre-Tiger Lake Intel GPUs (e.g. Haswell, Kaby Lake)
    - VC-1 is now decoded in software on these GPUs
  - Fix MPEG-2 streams starting very slowly or failing to start in time

## [0.2.3] - 2026-10-23
### Added
- Add optional `layout` to canvas graphics layers (playout schema `0.0.7`)
  - This allows compositing a canvas smaller than the output size
- Probe the first frame for HDR10 metadata when `probe_hint` omits `has_hdr10_metadata` for `smpte2084` video
  - This allows hardware tonemapping (QSV) for e.g. media server content that doesn't reliably report HDR10 metadata

### Changed
- VideoToolbox: no longer strip A53 closed captions from `h264` output
  - The workaround is not needed with ErsatzTV-ffmpeg `8.1.2-5`

### Fixed
- Fix channels falling behind schedule when the playout JSON is updated or replaced
- Fix image subtitles (e.g. PGS) missing or misplaced with hardware acceleration
  - This happened when the subtitle size differs from the video size (e.g. cropped video)
- Fix VAAPI stream failing when software `pad` is used before `tonemap_opencl`
- Fix QSV stream failing when no Intel device is found
  - The channel falls back to software
- Fix QSV green padding on Windows with certain HDR content
- Fix slow transcoding of high-resolution video with PGS subtitles until the first subtitle appears
- Fix RKMPP stream failing with loudness normalization
- Fix VAAPI blocky, smeared `h264` video on older AMD GPUs

## [0.2.2] - 2026-10-13
### Fixed
- Fix WebVTT cues that span HLS segments showing as several cues or twice
  - Cues keep their full start and end times in every segment, as RFC 8216 requires
- Only advertise a subtitle rendition in `/channel/{N}.m3u8` when `normalization.subtitle.mode` is `convert`
  - The rendition is no longer selected by default, and no longer claims English
- Fix WebVTT subtitle sources losing cue settings (position, alignment) in `convert` mode

## [0.2.1] - 2026-10-13
### Added
- Add `normalization.video.filters.scale.flags` (channel config schema `0.1.2`)
  - Chooses the software scaling algorithm (e.g. `bicubic`, `lanczos`, `fast_bilinear`) for the main picture, graphics and image subtitles
- Add `retry_budget_ms` and `retry_backoff_ms` to `dynamic` sources (playout schema `0.0.6`)
  - Retries a failed request with backoff; off when `retry_budget_ms` is absent or `0`
  - Transport errors, timeouts and 5xx responses are retried; 4xx responses and an invalid body are not
- Add `fallback.duration_seconds` (channel config schema `0.1.2`)
  - How long the fallback plays before the channel tries again (default 60)
  - After a failed `dynamic` source, the fallback lasts at least the source's `retry_budget_ms`

### Changed
- Use `bicubic` instead of `fast_bilinear` for software scaling by default
  - This reduces aliasing when downscaling
  - Set `normalization.video.filters.scale.flags` to `fast_bilinear` to keep the previous output
- Include the channel number in all `ersatztv-channel` log messages (`channel{number=7}`)
  - Set `ETV_LOG_FORMAT=json` for one JSON object per line, with the channel number in `span.number`

### Fixed
- Fix still images and song backgrounds encoded as 4:4:4 by software encoders, which many clients can't play
- Fix CUDA streams failing with a watermark in `yuva420p` (e.g. lossy WebP with transparency) and opacity below 100%
- Fix CUDA streams with a watermark failing when encoding with software (e.g. `mpeg2video`)
- Fix VideoToolbox `h264` streams failing on content with embedded (A53) closed captions
- Fix VAAPI `hevc` streams hanging on content with embedded (A53) closed captions with Mesa before 26.2.3
  - Captions are left out of `hevc` output on those Mesa versions; update Mesa to keep them
- Reject `normalization.video.filters` options that contain filter graph syntax when the channel config loads, instead of failing every item
- Fix `crop` scaling mode zooming in on content at least as large as the output (e.g. 4K on a 1080p channel) instead of scaling it to cover the output
- Fix sources without an audio stream showing the fallback for the whole item
  - These now play with silence, including items whose probe hint has no audio streams
- Fix `ersatztv-channel` transcoding forever when `.heartbeat` was never created
  - This happened when a worker spawned directly (not by `ersatztv`) never got a client
  - The 90 second timeout counts from when the channel became ready
  - Troubleshooting runs no longer stop on a stale `.heartbeat`
- Name the failed operation and file in `ersatztv-channel` disk errors, instead of `unable to load channel config (io)`
  - e.g. `failed to scan the segment folder .../5: No such file or directory`
  - A missing playout folder is no longer reported as a channel config problem
- Fix stream failing to black when an old segment was already deleted from disk
- Fix green output with legacy Intel QSV encoders

## [0.2.0] - 2026-10-07
### Breaking
- Replace `"format": null` with `"mode": "copy"` in `normalization.audio` and `normalization.video` (channel config schema `0.1.0`)
  - Channel configs and every overlay must declare `"version": "https://ersatztv.org/channel/version/0.1.0"` or newer
  - See [Updating Channel Configs to 0.1.0](https://ersatztv.org/next-docs/configuration/versions#updating-channel-configs-to-010)

### Added
- First versioned release; earlier builds report `0.1.0-<commit>`
- Release notes list the config versions each build reads and the [ErsatzTV-ffmpeg](https://github.com/ErsatzTV/ErsatzTV-ffmpeg) build it ships with

[Unreleased]: https://github.com/ErsatzTV/next/compare/v0.2.8...HEAD
[0.2.8]: https://github.com/ErsatzTV/next/compare/v0.2.7...v0.2.8
[0.2.7]: https://github.com/ErsatzTV/next/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/ErsatzTV/next/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/ErsatzTV/next/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/ErsatzTV/next/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/ErsatzTV/next/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/ErsatzTV/next/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/ErsatzTV/next/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/ErsatzTV/next/compare/v0.1.0...v0.2.0
