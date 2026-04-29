use std::time::Duration;

use crate::ArgVec;
use crate::audio_codec::AudioCodec;
use crate::frame_rate::FrameRate;
use crate::output_format::OutputFormat;
use crate::pipeline::{Hz, Kbps, OutputContext, PtsOffset};
use crate::video_codec::VideoCodec;

pub enum OutputOption {
    Format(OutputFormat),
    VideoCodec(VideoCodec),
    VideoBitrate(Option<Kbps>),
    VideoBuffer(Option<Kbps>),
    VideoTrackTimeScale(u64),
    AudioCodec(AudioCodec),
    AudioBitrate(Option<Kbps>),
    AudioBuffer(Option<Kbps>),
    AudioChannels(Option<u32>),
    AudioSampleRate(Option<Hz>),
    Duration(Duration),
    TsOffset(Option<PtsOffset>),
    CudaNoAutoScale,
    NoDemuxDecodeDelay,
    MovFlagsFastStart,
    DoNotMapMetadata,
    FrameRate(Option<FrameRate>),
}

impl OutputOption {
    pub(crate) fn as_arg(&self, output_context: &OutputContext) -> ArgVec {
        match self {
            OutputOption::Format(format) => format.as_arg(output_context),
            OutputOption::VideoCodec(codec) => codec.as_arg(),
            OutputOption::VideoBitrate(Some(bitrate_kbps)) => {
                args![
                    "-b:v",
                    format!("{}k", bitrate_kbps.0),
                    "-maxrate:v",
                    format!("{}k", bitrate_kbps.0),
                ]
            }
            OutputOption::VideoBitrate(None) => Vec::new(),
            OutputOption::VideoBuffer(Some(buffer_kbps)) => {
                args!["-bufsize:v", format!("{}k", buffer_kbps.0)]
            }
            OutputOption::VideoBuffer(None) => Vec::new(),
            OutputOption::VideoTrackTimeScale(time_scale) => {
                args!["-video_track_timescale", format!("{}", time_scale)]
            }
            OutputOption::AudioCodec(codec) => codec.as_arg(),
            OutputOption::AudioBitrate(Some(bitrate_kbps)) => {
                args![
                    "-b:a",
                    format!("{}k", bitrate_kbps.0),
                    "-maxrate:a",
                    format!("{}k", bitrate_kbps.0),
                ]
            }
            OutputOption::AudioBitrate(None) => Vec::new(),
            OutputOption::AudioBuffer(Some(buffer_kbps)) => {
                args!["-bufsize:a", format!("{}k", buffer_kbps.0)]
            }
            OutputOption::AudioBuffer(None) => Vec::new(),
            OutputOption::AudioChannels(Some(channels)) => {
                args!["-ac", format!("{}", channels)]
            }
            OutputOption::AudioChannels(None) => Vec::new(),
            OutputOption::AudioSampleRate(Some(sample_rate)) => {
                args!["-ar", format!("{}", sample_rate.0)]
            }
            OutputOption::AudioSampleRate(None) => Vec::new(),
            OutputOption::Duration(duration) => {
                args!["-t", format!("{}ms", duration.as_millis())]
            }
            OutputOption::TsOffset(Some(pts_offset)) if pts_offset.duration > Duration::ZERO => {
                args![
                    "-output_ts_offset",
                    format!("{}ms", pts_offset.duration.as_millis()),
                ]
            }
            OutputOption::TsOffset(_) => Vec::new(),
            OutputOption::CudaNoAutoScale => args!["-noautoscale"],
            OutputOption::NoDemuxDecodeDelay => args!["-muxdelay", "0", "-muxpreload", "0"],
            OutputOption::MovFlagsFastStart => {
                args!["-movflags", "+faststart"]
            }
            OutputOption::DoNotMapMetadata => {
                args!["-map_metadata", "-1"]
            }
            OutputOption::FrameRate(Some(frame_rate)) => {
                args!["-r", frame_rate.r_frame_rate.to_owned(), "-vsync", "cfr",]
            }
            OutputOption::FrameRate(_) => Vec::new(),
        }
    }
}
