# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0, the minor version marks breaking changes
(`0.B.C`, the same rule as the [config versions](https://ersatztv.org/next-docs/configuration/versions)).

## [Unreleased]
### Added
- VAAPI: tonemap with `libplacebo` when a Vulkan device on the same GPU is available
  - `libplacebo` is preferred over `tonemap_opencl` and `tonemap_vaapi`, and shows Dolby Vision profile 5 with correct colors
  - Padding right after the tonemap is drawn by `libplacebo` in the same pass instead of by a separate (or, on AMD, software) pad
  - Requires ErsatzTV-ffmpeg `8.1.2-6`
- Channel config `0.1.3`:
  - `normalization.video.vaapi_low_power` (`auto` or `never`, default `auto`)
  - `fallback.background_image`, a still image shown instead of black when there is nothing to play or an item fails
    - Black is used for the rest of the session if the image fails to play
- `/channels.m3u` includes each channel's number as `tvg-chno` and `channel-number`
  - Clients that number channels from the playlist (e.g. Channels DVR) no longer assign their own numbers

### Changed
- VAAPI: encode HEVC with the low-power entrypoint when the GPU also has the default entrypoint and low-power supports bitrate rate control
  - Set `normalization.video.vaapi_low_power` to `never` to keep the previous behavior
- AMF: decode MPEG-2 in software
  - Software decode is as fast or faster than AMF's MPEG-2 decoder, and avoids a hang on older (Polaris) GPUs

### Fixed
- Fix output being tagged with the source's non-square sample aspect ratio when the probe hint reports square pixels at the output size
  - Players showed the whole stream, watermark included, at the wrong aspect ratio (e.g. 16:9 squished to 4:3)
- Fix burning the wrong embedded text subtitle when a file has more than one subtitle stream
  - The first subtitle stream in the file was burned regardless of the selected stream index
- Fix VAAPI stream failing to start on AMD when an OpenCL runtime (e.g. NVIDIA, rusticl) is installed
  - `pad_opencl` and `tonemap_opencl` are now only used with an Intel VAAPI driver and an OpenCL device that supports `cl_intel_va_api_media_sharing`
- Fix QSV stream failing to start on Linux when the VPL runtime reports a decoder the VA driver has disabled (e.g. VC-1 and VP8 on Arc)
  - Those formats are now decoded in software
- Fix brief heavy blocking in VAAPI H.264 output with the `i965` driver after static scenes
  - H.264 encodes on `i965` now use a minimum QP of 24
- Fix QSV stream failing with `Can't allocate a surface` on pre-Tiger Lake Intel GPUs for files that store audio far behind video (e.g. some WMV)
- Fix AMF streams starting very slowly or failing to start in time due to improper segmentation
- Fix AMF streams hanging with loudness normalization when video is not scaled or converted on the GPU

## [0.2.3] - 2026-10-23
### Added
- Playout schema `0.0.7`: canvas graphics layers accept an optional `layout`
  - This can be used to composite a canvas smaller than the final output size 
- Probe for HDR10 metadata using first frame of local and HTTP sources, when `probe_hint` omits `has_hdr10_metadata` for `smpte2084` video
  - This allows hardware tonemapping (QSV) for e.g. media server content that doesn't reliably report HDR10 metadata

### Changed
- Remove VideoToolbox `h264` encoder workaround that stripped A53 closed captions
  - This is no longer needed with ErsatzTV-ffmpeg `8.1.2-5`

### Fixed
- Fix channels falling behind schedule when the playout JSON is updated or replaced
- Fix image subtitles (e.g. PGS) missing or misplaced with hardware acceleration when the subtitle size differs from the video size (e.g. cropped video)
- Fix VAAPI stream failing when software `pad` is used before `tonemap_opencl`
- Fix QSV stream failing when no Intel device is found; the channel now uses a software pipeline
- Fix QSV green padding on Windows with certain HDR content
- Fix slow transcoding of high-resolution video with PGS subtitles until the first subtitle appears
- Fix RKMPP stream failing with loudness normalization
- Fix VAAPI blocky, smeared `h264` video on older AMD GPUs

## [0.2.2] - 2026-10-13
### Fixed
- WebVTT cues that span HLS segments now keep their full start and end times in every segment, as RFC 8216 requires
  - Previously each segment cut the cue at its own boundaries, so players showed it as several cues or showed it twice
  - Cues still end with their item
- The channel playlist (`/channel/{N}.m3u8`) only advertises a subtitle rendition when `normalization.subtitle.mode` is `convert`
  - The rendition is no longer selected by default, and no longer claims English
- WebVTT subtitle sources keep their cue settings (position, alignment) in `convert` mode

## [0.2.1] - 2026-10-13
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
- Fix green output with legacy Intel QSV encoders

## [0.2.0] - 2026-10-07
### Breaking
- Channel config version `0.1.0` replaces `"format": null` with an explicit `"mode": "copy"` in `normalization.audio` and `normalization.video`
  - Channel configs and every overlay must declare `"version": "https://ersatztv.org/channel/version/0.1.0"` or newer
  - See [Updating Channel Configs to 0.1.0](https://ersatztv.org/next-docs/configuration/versions#updating-channel-configs-to-010)

### Added
- First versioned release; earlier builds report `0.1.0-<commit>`
- Release notes list the config versions each build reads and the [ErsatzTV-ffmpeg](https://github.com/ErsatzTV/ErsatzTV-ffmpeg) build it ships with

[Unreleased]: https://github.com/ErsatzTV/next/compare/v0.2.3...HEAD
[0.2.3]: https://github.com/ErsatzTV/next/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/ErsatzTV/next/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/ErsatzTV/next/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/ErsatzTV/next/compare/v0.1.0...v0.2.0
