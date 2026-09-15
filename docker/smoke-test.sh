#!/bin/sh
set -eu

platform=${1:?usage: smoke-test.sh PLATFORM APP_IMAGE TEST_IMAGE}
app_image=${2:?missing application image}
test_image=${3:?missing test image}

docker run --rm --platform "$platform" --entrypoint sh "$app_image" -ec '
    test "$(id -u)" = 1000
    test "$(id -g)" = 1000
    test -w /config
    /app/ersatztv --version
    /app/ersatztv-channel --version
    /app/ersatztv-playout-generator --version
    ffmpeg -hide_banner -loglevel error -f lavfi -i testsrc2=size=64x64:rate=10 \
        -t 1 -c:v libx264 -threads 1 /tmp/smoke.mp4
    ffprobe -v error -select_streams v:0 -show_entries stream=codec_name \
        -of default=noprint_wrappers=1:nokey=1 /tmp/smoke.mp4 | grep -x h264
'

# Run the Rust -> FFmpeg -> ffprobe path with a fixture file. On ARMv7 this
# also checks that the cross-compiled test binary runs under QEMU.
docker run --rm --platform "$platform" "$test_image" software codec_copy -- --exact
