# Test fixtures

Every file here is synthetic. `generate/generate.sh` recreates each one, and the
`fixture_properties` test in the software suite (`tests/common/fixtures.rs`) checks the
property each file exists to provide.

## Don't regenerate wholesale

The committed files are the source of truth. Leave them alone unless one is wrong or
you need a new property:

- Hardware suite results were collected against these exact bytes. A new x264 or x265
  build produces different files, so replacing them throws those results away.
- `ErsatzTV-ffmpeg/patches/manifests` pins copies of some fixtures by sha256.
- Several fixtures are only useful because of a quirk (see the table below), and a
  recipe that looks equivalent can lose it.

The recipes are for making a variant, or for replacing one file on purpose. A
regenerated file should pass `fixture_properties`. It doesn't need to match the
committed bytes.

## Recreating a fixture

```bash
# one fixture, or omit the names to recreate all of them (~20s)
FFMPEG=/path/to/etv/ffmpeg ./generate/generate.sh /tmp/fixtures 480i_h264_motion.ts

# check the properties of the regenerated set
ETV_TEST_FIXTURES=/tmp/fixtures cargo test --package ffpipeline --test software -- \
  --ignored --exact fixture_properties

# or run a whole suite against it
ETV_TEST_FIXTURES=/tmp/fixtures cargo test --package ffpipeline --test software -- \
  --ignored --test-threads 1
```

`FFMPEG` must be an ErsatzTV ffmpeg build (libx264, libx265, libsvtav1, libdav1d,
libwebp). The Dolby Vision fixtures also need the `x265` CLI, `dovi_tool` and GPAC's
`MP4Box`.

Checked against the committed files with ffmpeg n8.1.2-etv.6, x265 4.3, dovi_tool 2.3.4
and GPAC 26.07, the script gives:

- **byte-identical:** `1080p_hevc_10_bt2020_4x3.ts`, `1080p_hevc_10_hdr_scope.ts`, the
  `long_gop_*` .mp4/.ts files, `480p_h264_ac3_51.ts`, `subtitle*`, `watermark.*` and
  `background*`;
- **identical decoded frames:** the DV fixtures, `1080p_av1_10_hdr.mkv`,
  `1080p_h264_cropped_pgs.mkv`, `480i_h264_motion.ts`, `480i_mpeg2.ts`,
  `480p_h264_anamorphic.ts`, `480p_h264_video_only.mkv`, `480p_h264_10.ts`,
  `720p_h264_10.ts`, `720p_h264_audio_longer.mkv` and `long_gop_h264.mkv`;
- **same properties, different encode:** everything else. Those files came from other
  x264/x265 builds (the encoder settings are in their SEI), so the bytes differ but the
  source, settings and probed properties match.

## Adding a fixture

1. Add a `gen_<name>` function to `generate/generate.sh`.
2. List its distinguishing properties in `FIXTURES` in `tests/common/fixtures.rs`.
   `fixture_properties` fails for any file that isn't listed.
3. Add a row below.
4. Add it to the relevant shared or suite-specific test; fixtures are not automatically detected or tested.

## Fixtures

Unless noted, video is 2 s and audio is AAC. 8-bit sources are `testsrc2` at 30 fps with
a mono 440 Hz tone. 10-bit sources are `testsrc` at 24 fps with silent stereo.

| Fixture | What it provides | Used by |
|---|---|---|
| `1080p_h264.ts`, `720p_h264.ts`, `480p_h264.ts` | Baseline progressive H.264 at each output size. 1080p is coded at 1088 lines, so overlays that leak the padding rows fail the height check | `pipeline`, `watermark`, `crop`, copy tests and most others |
| `1080p_h264_10.ts`, `720p_h264_10.ts`, `480p_h264_10.ts` | H.264 High 10, which most hardware decoders can't decode | `pipeline` |
| `1080p_hevc_10.ts`, `720p_hevc_10.ts`, `480p_hevc_10.ts` | HEVC Main 10 SDR (no colour tags) | `pipeline`, `pad_color`, `watermark` |
| `480p_h264_anamorphic.ts` | 720x480 with SAR 32:27 (16:9 display), progressive | `pipeline`, `crop`, `stretch_anamorphic_same_size`, `watermark*` |
| `480p_h264_sps_change.ts` | The SPS changes mid-stream (level 3.0 to 5.1) without a size or format change. Two TS files joined, so timestamps are irregular and `r_frame_rate` probes as 240 | `pipeline`, `watermark` |
| `480p_h264_a53cc.ts` | ATSC A53 caption SEI on every frame. The cc_data is padding only (no caption text), written by `generate/insert_a53cc.py`. 1 kHz tone, 800 kb/s, no B-frames | `pipeline` |
| `480p_h264_ac3_51.ts` | AC-3 5.1(side) with one tone per channel: FL 250 Hz, FR 500, FC 1000, LFE 60, SL 2000, SR 4000, so a downmix or layout change shows up as a tone in the wrong channel | `surround_audio` |
| `480p_h264_video_only.mkv` | No audio stream | `video_only` |
| `720p_h264_rotated.mp4` | `720p_h264.ts` remuxed with a 90° display matrix | `rotated` |
| `720p_h264_audio_longer.mkv` | 4 s at 25 fps; audio runs 80 ms past the video | `watermark_periodic_audio_longer` |
| `480i_h264.ts` | Interlaced H.264, coded bottom field first even though ffprobe reports the stream as `tt` | `deinterlace` |
| `480i_h264_anamorphic.ts` | Interlaced (BFF) 720x480 with SAR 32:27 | `deinterlace` |
| `480i_h264_motion.ts` | TFF fields from 60p: an 81px bar that moves 8px per field on near-black, so dropped or woven fields leave combing that `motion_combing_score` measures | `deinterlace_motion`, `motion_check_rejects_missing_deinterlace` |
| `480i_mpeg2.ts` | Interlaced (TFF) MPEG-2, GOP 12, no B-frames | `deinterlace`, `copy_codec_*` |
| `1080i_h264.ts` | 16:9 interlaced (TFF), so no pad is needed and hardware frames can stay on the device | `interlaced_no_deinterlace`, `custom_frame_rate` |
| `1080p_hevc_10_hdr.ts` | HDR10 (PQ, BT.2020, mastering display and MaxCLL SEI) `smptehdbars`. Converted to the BT.2020 matrix, unlike the other bars fixtures | `tonemap_hdr*`, `custom_frame_rate` |
| `1080p_hevc_10_hdr_4x3.ts` | 1440x1080 centre cut of `1080p_hevc_10_hdr.ts`, audio copied: HDR that needs pillarboxing | `tonemap_hdr*`, `pad_color`, vaapi `pad_*` |
| `1080p_hevc_10_hdr_scope.ts` | 1920x800 HDR10: HDR that needs letterboxing | `pad_color`, vaapi `pad_*` |
| `1080p_hevc_10_bt2020_4x3.ts` | 1440x1080 BT.2020 SDR (`bt2020-10` transfer), which must not be tonemapped | `pad_color` |
| `1080p_av1_10_hdr.mkv` | AV1 Main 10 HDR10 (SVT-AV1), 1 kHz tone | `tonemap_hdr` |
| `1080p_hevc_10_dv5.mp4` | Dolby Vision profile 5 (IPT-C2, full range, no HDR10 fallback) | `tonemap_dv`, `copy_dv5` |
| `1080p_hevc_10_dv7.mp4` | Dolby Vision profile 7: HDR10 base layer plus a 960x540 enhancement layer carrying the RPU. dovi_tool reports the RPU itself as profile 8 | `tonemap_dv` |
| `1080p_hevc_10_dv81.mp4`, `_dv82.mp4`, `_dv84.mp4` | Dolby Vision 8.1 (HDR10), 8.2 (BT.709 SDR) and 8.4 (HLG) base layers | `tonemap_dv` |
| `long_gop_h264.mp4`, `.mkv`, `.ts` | 24 s, 256x144, one IDR every 150 frames (5 s), longer than an HLS segment. Same stream in three containers | `copy_seek`, `copy_input_start` |
| `long_gop_h264_open.ts`, `long_gop_hevc_open.ts` | As above with open GOPs: only the first keyframe is an IDR; later ones are recovery-point I frames (H.264) or CRAs (HEVC) | `copy_seek`, `copy_input_start` |
| `1080p_h264_cropped_pgs.mkv` | 8 s, 1920x1040 video with a 1920x1080 PGS track that starts at 6 s, past ffprobe's 5 s window, so probing can't find the subtitle size (#335). x264 veryslow, ref 16, level 5.1 | `image_subtitle_unknown_size` |
| `subtitle_pgs.sup` | PGS: one opaque white box from 0 to 10 s, written by `generate/make_pgs.py` | `copy_image_subtitle`, source for the cropped PGS fixture |
| `subtitle.srt` | One cue from 0 to 10 s | `copy_text_subtitle` |
| `watermark.png` | 200x200 `testsrc2` frame cut to a circle: RGBA with binary alpha | every watermark test (default) |
| `watermark.jpg` | `watermark.png` on grey: no alpha | `watermark_no_alpha` |
| `watermark.webp` | `watermark.png` as lossy WebP: yuva420p | `watermark_yuva` |
| `watermark.gif` | Animated, 10 frames at 10 fps, with transparency | `watermark_animated` |
| `background.png` | `background_rgb24.png` with a generated palette: pal8, a format `PixelFormat` doesn't model | `copy_still_image` |
| `background_rgb24.png` | `watermark.png` centred on opaque black at 1920x1080: rgb24 | `copy_still_image` |
| `background_alpha.png` | `watermark.png` padded to 1280x720 with transparent black | `copy_still_image_alpha` |
