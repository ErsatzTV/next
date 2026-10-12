# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0, the minor version marks breaking changes
(`0.B.C`, the same rule as the [config versions](https://ersatztv.org/next-docs/configuration/versions)).

## [Unreleased]
### Added
- Channel config version `0.1.2` adds `normalization.video.filters.scale.flags` to choose the software scaling algorithm (e.g. `bicubic`, `lanczos`, `fast_bilinear`)
  - Applies to the main picture, graphics and image subtitles wherever software scaling runs
- Playout version `0.0.6` adds `retry_budget_ms` and `retry_backoff_ms` to `dynamic` sources, to retry a failed request with backoff
  - Transport errors, timeouts and 5xx responses are retried; 4xx responses and an invalid body are not
  - Retry is off when `retry_budget_ms` is absent or `0`
- Channel config version `0.1.2` adds `fallback.duration_seconds` to set how long the fallback plays before the channel tries again (default 60)
  - After a failed `dynamic` source, the fallback lasts at least the source's `retry_budget_ms`

### Changed
- Software scaling now defaults to `bicubic` instead of `fast_bilinear`, which reduces aliasing when downscaling
  - Set `normalization.video.filters.scale.flags` to `fast_bilinear` to keep the previous output
- `ersatztv-channel` now logs through `tracing` and includes the channel number (`channel{number=7}`) in all messages
  - Set `ETV_LOG_FORMAT=json` for one JSON object per line with the channel number in `span.number`

### Fixed
- Always encode 4:2:0 output, which many clients require
  - Still images and song backgrounds were previously encoded as 4:4:4 by software encoders
- Fix CUDA streams failing with a watermark in `yuva420p` (e.g. lossy WebP with transparency) and opacity below 100%
- Fix CUDA streams with a watermark failing when encoding with software (e.g. `mpeg2video`)
- Fix VideoToolbox `h264` streams failing on content with embedded (A53) closed captions
- Fix VAAPI `hevc` streams hanging on content with embedded (A53) closed captions with Mesa before 26.2.3
  - Captions are left out of `hevc` output on those Mesa versions; update Mesa to keep them
- Reject `normalization.video.filters` options that contain filter graph syntax when the channel config loads, instead of failing every item
- Fix `crop` scaling mode cropping a zoomed-in window from content that is at least as large as the output (e.g. 4K on a 1080p channel) instead of scaling it to cover the output first
  - Anamorphic content is also converted to square pixels before the crop
- Play silence for sources without an audio stream, instead of the fallback for the whole item
  - Applies to every source kind, and to a probe hint with no audio streams
- `ersatztv-channel` now stops after 90 seconds without a client even when `.heartbeat` was never created, and counts from when the channel became ready
  - Previously a worker spawned directly (not by `ersatztv`) whose client never requested a segment transcoded forever
  - Troubleshooting runs no longer stop on a stale `.heartbeat`
- Disk errors in `ersatztv-channel` now name what failed and which file, instead of all reading `unable to load channel config (io)`
  - e.g. `failed to scan the segment folder .../5: No such file or directory`
  - A missing playout folder is no longer reported as a channel config problem
- No longer fail to black when an old segment that `ersatztv-channel` was trimming was already deleted from disk

## [0.2.0] - 2026-10-07
### Breaking
- Channel config version `0.1.0` replaces `"format": null` with an explicit `"mode": "copy"` in `normalization.audio` and `normalization.video`
  - Channel configs and every overlay must declare `"version": "https://ersatztv.org/channel/version/0.1.0"` or newer
  - See [Updating Channel Configs to 0.1.0](https://ersatztv.org/next-docs/configuration/versions#updating-channel-configs-to-010)

### Added
- First versioned release; earlier builds report `0.1.0-<commit>`
- Release notes list the config versions each build reads and the [ErsatzTV-ffmpeg](https://github.com/ErsatzTV/ErsatzTV-ffmpeg) build it ships with

[Unreleased]: https://github.com/ErsatzTV/next/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ErsatzTV/next/compare/v0.1.0...v0.2.0
