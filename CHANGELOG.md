# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0, the minor version marks breaking changes
(`0.B.C`, the same rule as the [config versions](https://ersatztv.org/next-docs/configuration/versions)).

## [Unreleased]
### Fixed
- Always encode 4:2:0 output, which many clients require
  - Still images and song backgrounds were previously encoded as 4:4:4 by software encoders
- Fix CUDA streams failing with a watermark in `yuva420p` (e.g. lossy WebP with transparency) and opacity below 100%
- Fix CUDA streams with a watermark failing when encoding with software (e.g. `mpeg2video`)
- Fix VideoToolbox `h264` streams failing on content with embedded (A53) closed captions

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
