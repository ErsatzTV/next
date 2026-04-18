use std::time::Duration;

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
    pub(crate) fn as_arg(&self, output_context: &OutputContext) -> Vec<String> {
        match self {
            OutputOption::Format(format) => format.as_arg(output_context),
            OutputOption::VideoCodec(codec) => codec.as_arg(),
            OutputOption::VideoBitrate(Some(bitrate_kbps)) => {
                vec![
                    String::from("-b:v"),
                    format!("{}k", bitrate_kbps.0),
                    String::from("-maxrate:v"),
                    format!("{}k", bitrate_kbps.0),
                ]
            }
            OutputOption::VideoBitrate(None) => Vec::new(),
            OutputOption::VideoBuffer(Some(buffer_kbps)) => {
                vec![String::from("-bufsize:v"), format!("{}k", buffer_kbps.0)]
            }
            OutputOption::VideoBuffer(None) => Vec::new(),
            OutputOption::AudioCodec(codec) => codec.as_arg(),
            OutputOption::AudioBitrate(Some(bitrate_kbps)) => {
                vec![
                    String::from("-b:a"),
                    format!("{}k", bitrate_kbps.0),
                    String::from("-maxrate:a"),
                    format!("{}k", bitrate_kbps.0),
                ]
            }
            OutputOption::AudioBitrate(None) => Vec::new(),
            OutputOption::AudioBuffer(Some(buffer_kbps)) => {
                vec![String::from("-bufsize:a"), format!("{}k", buffer_kbps.0)]
            }
            OutputOption::AudioBuffer(None) => Vec::new(),
            OutputOption::AudioChannels(Some(channels)) => {
                vec![String::from("-ac"), format!("{}", channels)]
            }
            OutputOption::AudioChannels(None) => Vec::new(),
            OutputOption::AudioSampleRate(Some(sample_rate)) => {
                vec![String::from("-ar"), format!("{}", sample_rate.0)]
            }
            OutputOption::AudioSampleRate(None) => Vec::new(),
            OutputOption::Duration(duration) => {
                vec![String::from("-t"), format!("{}ms", duration.as_millis())]
            }
            OutputOption::TsOffset(Some(pts_offset)) if pts_offset.duration > Duration::ZERO => {
                vec![
                    String::from("-output_ts_offset"),
                    format!("{}ms", pts_offset.duration.as_millis()),
                ]
            }
            OutputOption::TsOffset(_) => Vec::new(),
            OutputOption::CudaNoAutoScale => vec![String::from("-noautoscale")],
            OutputOption::NoDemuxDecodeDelay => vec!["-muxdelay", "0", "-muxpreload", "0"]
                .into_iter()
                .map(String::from)
                .collect(),
            OutputOption::MovFlagsFastStart => {
                vec![String::from("-movflags"), String::from("+faststart")]
            }
            OutputOption::DoNotMapMetadata => {
                vec![String::from("-map_metadata"), String::from("-1")]
            }
            OutputOption::FrameRate(Some(frame_rate)) => {
                vec![
                    String::from("-r"),
                    frame_rate.r_frame_rate.to_owned(),
                    String::from("-vsync"),
                    String::from("cfr"),
                ]
            }
            OutputOption::FrameRate(_) => Vec::new(),
        }
    }
}
