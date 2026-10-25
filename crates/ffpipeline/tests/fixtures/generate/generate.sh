#!/usr/bin/env bash
# shellcheck disable=SC2046  # tone, silence and long_gop_src expand to several ffmpeg args
# Recreates the test fixtures. See ../README.md before replacing a committed one.
#
# usage: generate.sh <out-dir> [fixture ...]   (no fixture names: all of them)
#
# FFMPEG must be an ErsatzTV ffmpeg build (libx264, libx265, libsvtav1, libdav1d, libwebp).
# The Dolby Vision fixtures also need the x265 CLI, dovi_tool and GPAC's MP4Box.
set -euo pipefail

FFMPEG=${FFMPEG:-ffmpeg}
X265=${X265:-x265}
DOVI_TOOL=${DOVI_TOOL:-dovi_tool}
MP4BOX=${MP4BOX:-MP4Box}
here=$(cd "$(dirname "$0")" && pwd)

[ $# -ge 1 ] || { sed -n 4p "$0"; exit 2; }
out=$(mkdir -p "$1" && cd "$1" && pwd)
shift
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT

ff() { "$FFMPEG" -nostdin -hide_banner -loglevel error -y "$@"; }

tone() { echo -f lavfi -i "sine=f=${2:-440}:r=48000:d=$1"; }
silence() { echo -f lavfi -i "anullsrc=r=48000:cl=stereo:d=$1"; }

# 8-bit progressive h264: testsrc2, 30 fps, x264 defaults, mono tone
progressive_h264() { # name size
    ff -f lavfi -i "testsrc2=s=$2:r=30:d=2" $(tone 2) \
        -c:v libx264 -c:a aac -ac 1 "$out/$1"
}

# 10-bit sources: testsrc, 24 fps, silent stereo
ten_bit() { # name size codec
    ff -f lavfi -i "testsrc=s=$2:r=24:d=2" $(silence 2) \
        -c:v "$3" -pix_fmt yuv420p10le -c:a aac -shortest "$out/$1"
}

# smptehdbars HEVC Main 10 with x265 params, 24 fps, mono tone
bars_hevc() { # name size x265-params [preset [audio-bitrate [filters]]]
    ff -f lavfi -i "smptehdbars=s=$2:r=24" $(tone 2) -t 2 ${6:+-vf $6} \
        -c:v libx265 -preset "${4:-ultrafast}" -crf 28 -pix_fmt yuv420p10le \
        -x265-params "log-level=0:$3" -c:a aac -ac 1 ${5:+-b:a $5} -f mpegts "$out/$1"
}

BT2020="colorprim=bt2020:colormatrix=bt2020nc:range=limited"
HDR10="$BT2020:transfer=smpte2084:master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,1):max-cll=1000,400:hdr10=1"

gen_1080p_h264.ts() { progressive_h264 1080p_h264.ts 1920x1080; }
gen_720p_h264.ts() { progressive_h264 720p_h264.ts 1280x720; }
gen_480p_h264.ts() { progressive_h264 480p_h264.ts 640x480; }

gen_1080p_h264_10.ts() { ten_bit 1080p_h264_10.ts 1920x1080 libx264; }
gen_720p_h264_10.ts() { ten_bit 720p_h264_10.ts 1280x720 libx264; }
gen_480p_h264_10.ts() { ten_bit 480p_h264_10.ts 640x480 libx264; }
gen_1080p_hevc_10.ts() { ten_bit 1080p_hevc_10.ts 1920x1080 libx265; }
gen_720p_hevc_10.ts() { ten_bit 720p_hevc_10.ts 1280x720 libx265; }
gen_480p_hevc_10.ts() { ten_bit 480p_hevc_10.ts 640x480 libx265; }

gen_480p_h264_anamorphic.ts() {
    ff -f lavfi -i "testsrc=s=720x480:r=30:d=2" $(silence 2) -vf setsar=32/27 \
        -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest "$out/480p_h264_anamorphic.ts"
}

gen_480p_h264_sps_change.ts() {
    # 15 frames at level 3.0, then 30 at 5.1: the SPS changes without a size or format change
    local list=$work/sps_change.txt
    ff -f lavfi -i "testsrc2=s=640x480:r=30:d=0.5" $(silence 0.5) \
        -c:v libx264 -level 3.0 -g 15 -c:a aac -shortest "$work/sps_0.ts"
    ff -f lavfi -i "testsrc2=s=640x480:r=30:d=1" $(silence 1) \
        -c:v libx264 -level 5.1 -g 15 -c:a aac -shortest "$work/sps_1.ts"
    printf "file '%s'\n" "$work/sps_0.ts" "$work/sps_1.ts" > "$list"
    ff -f concat -safe 0 -i "$list" -c copy "$out/480p_h264_sps_change.ts"
}

gen_480p_h264_a53cc.ts() {
    ff -f lavfi -i "testsrc2=s=640x480:r=30:d=2" \
        -c:v libx264 -preset veryfast -b:v 800k -g 30 -bf 0 -f h264 "$work/a53.h264"
    python3 "$here/insert_a53cc.py" "$work/a53.h264" "$work/a53cc.h264"
    # raw h264 has no timestamps; the mkv muxer generates them so -shortest works
    ff -r 30 -f h264 -i "$work/a53cc.h264" -c copy "$work/a53cc.mkv"
    ff -i "$work/a53cc.mkv" $(tone 2 1000) -map 0:v -map 1:a -c:v copy -c:a aac -ac 1 -shortest \
        "$out/480p_h264_a53cc.ts"
}

# one tone per channel at sine's default amplitude, so downmix and layout routing can be measured.
# aevalsrc rather than join: join of several sine inputs isn't deterministic at EOF.
tones() { # layout hz...
    local layout=$1 expr=""
    shift
    for hz in "$@"; do expr+="|sin(2*PI*$hz*t)/8"; done
    echo -f lavfi -i "aevalsrc=${expr#|}:c=$layout:s=48000:d=2"
}

gen_480p_h264_ac3_51.ts() {
    ff -f lavfi -i "testsrc2=s=640x480:r=30:d=2" $(tones "5.1(side)" 250 500 1000 60 2000 4000) \
        -c:v libx264 -c:a ac3 -b:a 384k "$out/480p_h264_ac3_51.ts"
}

# the audio is what these are for, so the video is cheap
# bitexact: matroska otherwise writes a random segment UID
audio_codec() { # name layout codec-args...
    local name=$1 layout=$2 hz=(250 500 1000 60 2000 4000)
    shift 2
    [ "$layout" = stereo ] && hz=(250 500)
    ff -f lavfi -i "testsrc2=s=640x480:r=30:d=2" $(tones "$layout" "${hz[@]}") \
        -c:v libx264 -crf 35 "$@" -fflags +bitexact "$out/$name"
}

gen_480p_h264_eac3_51.mkv() { audio_codec 480p_h264_eac3_51.mkv "5.1(side)" -c:a eac3 -b:a 384k; }
# dca and truehd are experimental encoders: DTS core only (no DTS-HD MA), TrueHD without Atmos
gen_480p_h264_dts_51.mkv() { audio_codec 480p_h264_dts_51.mkv "5.1(side)" -c:a dca -strict -2 -b:a 768k; }
gen_480p_h264_truehd_51.mkv() { audio_codec 480p_h264_truehd_51.mkv "5.1(side)" -c:a truehd -strict -2; }
# libopus's surround mapping only takes the back layout
gen_480p_h264_opus_51.mkv() {
    audio_codec 480p_h264_opus_51.mkv 5.1 -c:a libopus -b:a 256k -mapping_family 1
}
gen_480p_h264_flac.mkv() { audio_codec 480p_h264_flac.mkv stereo -c:a flac; }
gen_480p_h264_mp3.mkv() { audio_codec 480p_h264_mp3.mkv stereo -c:a libmp3lame -b:a 192k; }
gen_480p_h264_pcm_s24le.mkv() { audio_codec 480p_h264_pcm_s24le.mkv stereo -c:a pcm_s24le; }

gen_480p_h264_video_only.mkv() {
    ff -f lavfi -i "testsrc2=s=640x480:r=30:d=2" -c:v libx264 "$out/480p_h264_video_only.mkv"
}

gen_720p_h264_rotated.mp4() {
    [ -f "$out/720p_h264.ts" ] || gen_720p_h264.ts
    ff -display_rotation 90 -i "$out/720p_h264.ts" -c copy "$out/720p_h264_rotated.mp4"
}

gen_720p_h264_audio_longer.mkv() {
    ff -f lavfi -i "testsrc2=s=1280x720:r=25:d=4" -f lavfi -i "sine=f=440:r=48000:d=4.08" \
        -ac 2 -c:v libx264 -preset veryfast -profile:v high -pix_fmt yuv420p -b:v 400k \
        -g 25 -keyint_min 25 -sc_threshold 0 -c:a aac -b:a 64k -map_metadata -1 \
        "$out/720p_h264_audio_longer.mkv"
}

interlaced_h264() { # name src size top preset audio...
    local name=$1 src=$2 size=$3 top=$4 preset=$5
    shift 5
    ff -f lavfi -i "$src=s=$size:r=30:d=2" "$@" \
        -c:v libx264 -preset "$preset" -pix_fmt yuv420p -b:v 1500k -flags +ildct+ilme -top "$top" \
        -c:a aac -shortest "$out/$name"
}

# -top 0: x264 codes bottom field first, although ffprobe reports the stream as tt
gen_480i_h264.ts() { interlaced_h264 480i_h264.ts testsrc2 640x480 0 medium $(tone 2) -ac 1; }
gen_480i_h264_anamorphic.ts() {
    interlaced_h264 480i_h264_anamorphic.ts testsrc 720x480 0 medium $(silence 2) -vf setsar=32/27
}
gen_1080i_h264.ts() { interlaced_h264 1080i_h264.ts testsrc2 1920x1080 1 veryfast $(tone 2) -ac 1; }

gen_480i_h264_motion.ts() {
    # a bar that moves 8px per field, so a deinterlacer that drops or weaves fields leaves combing
    # interleave_top takes its first field from frame 1
    ff -f lavfi -i "color=c=0x040404:s=640x480:r=60:d=2" -f lavfi -i "color=c=0xe6e6e6:s=81x480:r=60" \
        $(tone 2) -filter_complex "[0][1]overlay=x=40+8*mod(n-1\,60):y=0:shortest=1,scale=in_range=pc:out_range=tv,tinterlace=interleave_top" \
        -map 2:a -c:v libx264 -preset veryfast -crf 18 -flags +ildct+ilme -top 1 \
        -c:a aac -ac 1 -shortest "$out/480i_h264_motion.ts"
}

gen_480i_mpeg2.ts() {
    ff -f lavfi -i "testsrc2=s=640x480:r=30:d=2" $(tone 2) \
        -c:v mpeg2video -b:v 4M -g 12 -bf 0 -flags +ildct+ilme -top 1 \
        -c:a aac -ac 1 -shortest "$out/480i_mpeg2.ts"
}

# Source codecs other than h264/hevc, each in the container and with the audio it usually
# comes with. Quality is low to keep the files small; the ssim check compares against
# the fixture, not the pattern.

# ATSC broadcast: 29.97i and 59.94p MPEG-2 with B-frames, AC-3 stereo.
# testsrc rather than testsrc2: MPEG-2 can't make testsrc2 small.
gen_1080i_mpeg2.ts() {
    ff -f lavfi -i "testsrc=s=1920x1080:r=30000/1001:d=2" $(tone 2) \
        -c:v mpeg2video -q:v 4 -g 15 -bf 2 -flags +ildct+ilme -top 1 \
        -c:a ac3 -ac 2 "$out/1080i_mpeg2.ts"
}
gen_720p_mpeg2.ts() {
    ff -f lavfi -i "testsrc=s=1280x720:r=60000/1001:d=2" $(tone 2) \
        -c:v mpeg2video -q:v 4 -g 15 -bf 2 -c:a ac3 -ac 2 "$out/720p_mpeg2.ts"
}

# Xvid (MPEG-4 Part 2 ASP) with B-frames and MP3 in AVI. ffmpeg can't write DivX packed B-frames.
gen_480p_mpeg4.avi() {
    ff -f lavfi -i "testsrc2=s=640x480:r=30000/1001:d=2" $(tone 2) \
        -c:v libxvid -q:v 8 -bf 2 -vtag XVID -c:a libmp3lame -ac 1 -b:a 64k "$out/480p_mpeg4.avi"
}

# bitexact: matroska and webm otherwise write a random segment UID
vp9() { # name pattern rate pix_fmt
    ff -f lavfi -i "$2=s=1920x1080:r=$3:d=2" $(tone 2) \
        -c:v libvpx-vp9 -pix_fmt "$4" -crf 55 -b:v 0 -cpu-used 4 -c:a libopus -ac 1 \
        -fflags +bitexact "$out/$1"
}
gen_1080p_vp9.webm() { vp9 1080p_vp9.webm testsrc2 30 yuv420p; }
# profile 2, like the other 10-bit sources: testsrc at 24 fps, no colour tags
gen_1080p_vp9_10.webm() { vp9 1080p_vp9_10.webm testsrc 24 yuv420p10le; }

gen_1080p_av1.mkv() {
    ff -f lavfi -i "testsrc2=s=1920x1080:r=30:d=2" $(tone 2) \
        -c:v libsvtav1 -preset 8 -crf 55 -c:a aac -ac 1 -fflags +bitexact "$out/1080p_av1.mkv" \
        2>&1 | { grep -v ^Svt || true; }
}

# frame rates outside the 30 fps family
gen_1080p_h264_23976.mkv() {
    ff -f lavfi -i "testsrc2=s=1920x1080:r=24000/1001:d=2" $(tone 2) \
        -c:v libx264 -crf 35 -c:a aac -ac 1 -fflags +bitexact "$out/1080p_h264_23976.mkv"
}
gen_720p_h264_50.ts() {
    ff -f lavfi -i "testsrc2=s=1280x720:r=50:d=2" $(tone 2) \
        -c:v libx264 -crf 35 -c:a aac -ac 1 "$out/720p_h264_50.ts"
}
# PAL DVD: 16:9 anamorphic 576i25 with MP2 audio. testsrc, since MPEG-2 can't make testsrc2 small.
gen_576i_mpeg2.ts() {
    ff -f lavfi -i "testsrc=s=720x576:r=25:d=2" $(tone 2) -vf setsar=64/45 \
        -c:v mpeg2video -q:v 4 -g 12 -bf 2 -flags +ildct+ilme -top 1 \
        -c:a mp2 -ac 2 -b:a 192k "$out/576i_mpeg2.ts"
}

# sizes that don't divide evenly: a cropped scope encode, and 4:3 NTSC (ITU SAR 10:11),
# whose display width (654.5) isn't a whole number
gen_1080p_h264_1916x796.mkv() {
    ff -f lavfi -i "testsrc2=s=1916x796:r=24000/1001:d=2" $(tone 2) \
        -c:v libx264 -crf 35 -c:a aac -ac 1 -fflags +bitexact "$out/1080p_h264_1916x796.mkv"
}
gen_480p_h264_sar_10x11.ts() {
    ff -f lavfi -i "testsrc2=s=720x480:r=30000/1001:d=2" $(tone 2) -vf setsar=10/11 \
        -c:v libx264 -crf 25 -c:a aac -ac 1 "$out/480p_h264_sar_10x11.ts"
}

# testsrc is rgb, which x264 would keep as 4:4:4
gen_2160p_h264.ts() {
    ff -f lavfi -i "testsrc=s=3840x2160:r=30:d=2" $(tone 2) \
        -c:v libx264 -pix_fmt yuv420p -level 5.1 -crf 30 -c:a aac -ac 1 "$out/2160p_h264.ts"
}

# BT.2020 matrix from swscale; the other bars fixtures keep smptehdbars' BT.709 values
gen_1080p_hevc_10_hdr.ts() {
    bars_hevc 1080p_hevc_10_hdr.ts 1920x1080 "$HDR10" medium 128k scale=out_color_matrix=bt2020
}
gen_1080p_hevc_10_hdr_4x3.ts() {
    # centre cut of the 16:9 fixture, audio copied
    [ -f "$out/1080p_hevc_10_hdr.ts" ] || gen_1080p_hevc_10_hdr.ts
    ff -i "$out/1080p_hevc_10_hdr.ts" -vf crop=1440:1080 -c:v libx265 -preset ultrafast -crf 28 \
        -pix_fmt yuv420p10le -x265-params "log-level=0:$HDR10" -c:a copy "$out/1080p_hevc_10_hdr_4x3.ts"
}
gen_1080p_hevc_10_hdr_scope.ts() { bars_hevc 1080p_hevc_10_hdr_scope.ts 1920x800 "$HDR10"; }
gen_1080p_hevc_10_bt2020_4x3.ts() {
    bars_hevc 1080p_hevc_10_bt2020_4x3.ts 1440x1080 "$BT2020:transfer=bt2020-10"
}
gen_1080p_hevc_10_hlg.ts() {
    bars_hevc 1080p_hevc_10_hlg.ts 1920x1080 "$BT2020:transfer=arib-std-b67" ultrafast "" \
        scale=out_color_matrix=bt2020
}
gen_2160p_hevc_10_hdr.ts() {
    bars_hevc 2160p_hevc_10_hdr.ts 3840x2160 "$HDR10" ultrafast "" scale=out_color_matrix=bt2020
}

gen_1080p_av1_10_hdr.mkv() {
    local svt="mastering-display=G(0.265,0.69)B(0.15,0.06)R(0.68,0.32)WP(0.3127,0.329)L(1000,0.005):content-light=1000,400:enable-hdr=1"
    local enc=(-pix_fmt yuv420p10le -c:v libsvtav1 -preset 8 -crf 35 -g 48 -svtav1-params "$svt")
    ff -f lavfi -i "smptehdbars=s=1920x1080:r=24:d=2" \
        -vf setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc:range=tv \
        "${enc[@]}" "$work/av1.obu" 2>&1 | { grep -v ^Svt || true; }
    # two passes only because that's how the committed file was made; one gives the same tags
    ff -framerate 24 -c:v libdav1d -i "$work/av1.obu" $(tone 2 1000) \
        "${enc[@]}" -c:a aac -ac 1 -shortest "$out/1080p_av1_10_hdr.mkv" 2>&1 | { grep -v ^Svt || true; }
}

dv_source() {
    [ -f "$work/dv.y4m" ] && return
    # a moving box gives the encoder (and a tonemapped output) motion to work with
    ff -f lavfi -i "smptehdbars=s=1920x1080:r=24:d=2" -f lavfi -i "color=white:s=120x120:r=24" \
        -filter_complex "[0][1]overlay=x=t*400:y=900:shortest=1,format=yuv420p10le" \
        -strict -1 "$work/dv.y4m"
    ff -f lavfi -i "sine=f=440:r=48000:d=2" -ac 2 -c:a aac -b:a 128k "$work/dv.m4a"
}

dv_rpu() { # generator profile
    [ -f "$work/dv$1.rpu" ] && return
    cat > "$work/dv$1.json" <<EOF
{
  "cm_version": "V29",
  "profile": "$1",
  "length": 48,
  "level6": {
    "max_display_mastering_luminance": 1000,
    "min_display_mastering_luminance": 1,
    "max_content_light_level": 1000,
    "max_frame_average_light_level": 400
  }
}
EOF
    "$DOVI_TOOL" generate -j "$work/dv$1.json" -o "$work/dv$1.rpu" > /dev/null
}

dv_x265() { # out.hevc args...
    local dst=$1
    shift
    "$X265" --input "$work/dv.y4m" --output-depth 10 --log-level error --no-progress \
        --crf 30 --keyint 24 --min-keyint 24 --no-open-gop --repeat-headers \
        --vbv-maxrate 2000 --vbv-bufsize 2000 "$@" -o "$dst"
}

DV_HDR10=(--colorprim bt2020 --transfer smpte2084 --colormatrix bt2020nc
    --master-display "G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1)"
    --max-cll "1000,400")

dv_mux() { # name hevc dvp
    rm -f "$out/$1"
    "$MP4BOX" -quiet -add "$2:dvp=$3:fps=24" -add "$work/dv.m4a" -new "$out/$1" > /dev/null
}

gen_1080p_hevc_10_dv5.mp4() {
    dv_source && dv_rpu 5
    dv_x265 "$work/dv5.hevc" --range full --colorprim bt2020 --transfer smpte2084 \
        --colormatrix ipt-pq-c2 --dolby-vision-profile 5 --dolby-vision-rpu "$work/dv5.rpu"
    dv_mux 1080p_hevc_10_dv5.mp4 "$work/dv5.hevc" f5
}

gen_1080p_hevc_10_dv81.mp4() {
    dv_source && dv_rpu 8.1
    dv_x265 "$work/dv81.hevc" "${DV_HDR10[@]}" --dolby-vision-profile 8.1 \
        --dolby-vision-rpu "$work/dv8.1.rpu"
    dv_mux 1080p_hevc_10_dv81.mp4 "$work/dv81.hevc" 8.hdr10
}

gen_1080p_hevc_10_dv82.mp4() {
    dv_source && dv_rpu 8.1
    dv_x265 "$work/dv82.hevc" --colorprim bt709 --transfer bt709 --colormatrix bt709 \
        --dolby-vision-profile 8.2 --dolby-vision-rpu "$work/dv8.1.rpu"
    dv_mux 1080p_hevc_10_dv82.mp4 "$work/dv82.hevc" 8.bt709
}

gen_1080p_hevc_10_dv84.mp4() {
    # x265 has no 8.4 mode, so the RPU is injected afterwards
    dv_source && dv_rpu 8.4
    dv_x265 "$work/dv84_bl.hevc" --colorprim bt2020 --transfer arib-std-b67 --colormatrix bt2020nc \
        --aud --hrd
    "$DOVI_TOOL" inject-rpu -i "$work/dv84_bl.hevc" --rpu-in "$work/dv8.4.rpu" \
        -o "$work/dv84.hevc" > /dev/null
    dv_mux 1080p_hevc_10_dv84.mp4 "$work/dv84.hevc" 8.hlg2100
}

gen_1080p_hevc_10_dv7.mp4() {
    # dual layer: HDR10 base layer plus a 960x540 enhancement layer carrying the RPU
    dv_source && dv_rpu 8.1
    dv_x265 "$work/dv7_bl.hevc" "${DV_HDR10[@]}" --hdr10
    ff -i "$work/dv.y4m" -vf scale=960:540 -strict -1 "$work/dv_el.y4m"
    "$X265" --input "$work/dv_el.y4m" --output-depth 10 --log-level error --no-progress --crf 40 \
        --keyint 24 --min-keyint 24 --no-open-gop --repeat-headers -o "$work/dv7_el.hevc"
    "$DOVI_TOOL" inject-rpu -i "$work/dv7_el.hevc" --rpu-in "$work/dv8.1.rpu" \
        -o "$work/dv7_el_rpu.hevc" > /dev/null
    "$DOVI_TOOL" mux --bl "$work/dv7_bl.hevc" --el "$work/dv7_el_rpu.hevc" \
        -o "$work/dv7.hevc" > /dev/null
    dv_mux 1080p_hevc_10_dv7.mp4 "$work/dv7.hevc" f7.brd
}

long_gop_src() { echo -f lavfi -i "testsrc2=s=256x144:r=30000/1001:d=24" $(tone 24); }

gen_long_gop_h264.mp4() {
    # one keyframe per 150 frames (5 s): longer than an HLS segment
    ff $(long_gop_src) -c:v libx264 -preset veryfast -crf 42 -g 150 -bf 3 \
        -keyint_min 150 -sc_threshold 0 -c:a aac -b:a 16k -ac 1 -shortest "$out/long_gop_h264.mp4"
}
gen_long_gop_h264.mkv() {
    [ -f "$out/long_gop_h264.mp4" ] || gen_long_gop_h264.mp4
    ff -i "$out/long_gop_h264.mp4" -c copy "$out/long_gop_h264.mkv"
}
gen_long_gop_h264.ts() {
    [ -f "$out/long_gop_h264.mp4" ] || gen_long_gop_h264.mp4
    ff -i "$out/long_gop_h264.mp4" -c copy "$out/long_gop_h264.ts"
}
gen_long_gop_h264_open.ts() {
    ff $(long_gop_src) -c:v libx264 -preset veryfast -crf 42 -g 150 -bf 3 \
        -x264-params open-gop=1 -c:a aac -b:a 16k -ac 1 -shortest "$work/open.mp4"
    ff -i "$work/open.mp4" -c copy "$out/long_gop_h264_open.ts"
}
gen_long_gop_hevc_open.ts() {
    ff $(long_gop_src) -c:v libx265 -preset veryfast -crf 38 \
        -x265-params "keyint=150:min-keyint=150:scenecut=0:bframes=3:open-gop=1:log-level=error" \
        -c:a aac -b:a 16k -ac 1 -shortest -f mpegts "$out/long_gop_hevc_open.ts"
}

gen_subtitle_pgs.sup() { python3 "$here/make_pgs.py" "$out/subtitle_pgs.sup"; }

gen_subtitle.srt() {
    printf '1\n00:00:00,000 --> 00:00:10,000\nCopy channel burned subtitle\n' > "$out/subtitle.srt"
}

gen_1080p_h264_cropped_pgs.mkv() {
    [ -f "$out/subtitle_pgs.sup" ] || gen_subtitle_pgs.sup
    # cropped (1920x1040) video with a 1920x1080 PGS track whose first display set is
    # past ffprobe's 5s window, so probing can't find the subtitle size (#335)
    ff -f lavfi -i "color=c=0x1f3a93:s=1920x1040:r=24000/1001:d=8" -f lavfi -i "sine=f=440:d=8:r=48000" \
        -itsoffset 6 -i "$out/subtitle_pgs.sup" -map 0:v -map 1:a -map 2:s \
        -c:v libx264 -preset veryslow -crf 23 -g 24 -pix_fmt yuv420p \
        -c:a aac -b:a 64k -ac 2 -c:s copy -t 8 "$out/1080p_h264_cropped_pgs.mkv"
}

gen_watermark.png() {
    # testsrc2 cut to a circle: a binary alpha channel
    ff -f lavfi -i "testsrc2=s=200x200:d=1" \
        -vf "format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='if(lt(hypot(X-100,Y-100),95),255,0)'" \
        -frames:v 1 "$out/watermark.png"
}
gen_watermark.jpg() {
    [ -f "$out/watermark.png" ] || gen_watermark.png
    ff -f lavfi -i color=gray:s=200x200 -i "$out/watermark.png" \
        -filter_complex "[0][1]overlay,format=yuvj420p" -frames:v 1 -q:v 3 "$out/watermark.jpg"
}
gen_watermark.webp() {
    [ -f "$out/watermark.png" ] || gen_watermark.png
    ff -i "$out/watermark.png" -c:v libwebp -quality 80 "$out/watermark.webp"
}
gen_watermark.gif() {
    # animated, 10 frames at 10 fps
    ff -f lavfi -i "testsrc2=s=120x120:r=10:d=1" \
        -vf "format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='if(lt(hypot(X-60,Y-60),55),255,0)',split[a][b];[a]palettegen=reserve_transparent=1[p];[b][p]paletteuse=alpha_threshold=128" \
        "$out/watermark.gif"
}

gen_background_rgb24.png() {
    [ -f "$out/watermark.png" ] || gen_watermark.png
    ff -f lavfi -i color=black:s=1920x1080 -i "$out/watermark.png" \
        -filter_complex "[0][1]overlay=(W-w)/2:(H-h)/2,format=rgb24" -frames:v 1 \
        "$out/background_rgb24.png"
}
gen_background.png() {
    [ -f "$out/background_rgb24.png" ] || gen_background_rgb24.png
    # swscale's pal8 conversion dithers the flat black into noise; a generated palette doesn't
    ff -i "$out/background_rgb24.png" -vf "split[a][b];[a]palettegen[p];[b][p]paletteuse=dither=none" \
        "$out/background.png"
}
gen_background_alpha.png() {
    [ -f "$out/watermark.png" ] || gen_watermark.png
    ff -i "$out/watermark.png" -vf "pad=1280:720:-1:-1:color=black@0" "$out/background_alpha.png"
}

if [ $# -eq 0 ]; then
    set -- $(declare -F | awk '{print $3}' | sed -n 's/^gen_//p')
fi
for name in "$@"; do
    declare -F "gen_$name" > /dev/null || { echo "unknown fixture: $name" >&2; exit 2; }
    echo "$name"
    "gen_$name"
done
