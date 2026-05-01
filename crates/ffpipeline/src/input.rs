use std::time::Duration;

use enum_dispatch::enum_dispatch;
use simple_expand_tilde::expand_tilde;

use crate::ArgVec;
use crate::error::FFPipelineError;
use crate::frame_size::FrameSize;
use crate::overlay_filter::FramePoint;
use crate::probe::{
    CodecType, ProbeResult, ProbeResultAudioStream, ProbeResultStream, ProbeResultVideoStream,
};

pub struct InputSettings {
    pub audio_input: ProbedInput,
    pub video_input: ProbedInput,
    pub subtitle_input: Option<ProbedInput>,
    pub watermark_input: Option<WatermarkInput>,
}

impl InputSettings {
    pub fn select_video_stream(&self) -> Result<&ProbeResultVideoStream, FFPipelineError> {
        let mut all_video_streams: Vec<&Box<ProbeResultVideoStream>> = self
            .video_input
            .probe_result
            .streams
            .iter()
            .filter_map(|s| match s {
                ProbeResultStream::Video(video_stream)
                    if video_stream.codec_type == CodecType::Video =>
                {
                    Some(video_stream)
                }
                _ => None,
            })
            .collect();

        if let Some(video_index) = self.video_input.stream_index {
            let matched_stream = all_video_streams
                .iter()
                .find(|v| v.stream_index == video_index);

            match matched_stream {
                Some(video_stream) => {
                    return Ok(video_stream);
                }
                None => {
                    log::warn!(
                        "unable to locate requested video stream with index {}",
                        video_index
                    );
                }
            }
        }

        match all_video_streams.len() {
            0 => Err(FFPipelineError::VideoInputIsRequired),
            1 => Ok(all_video_streams[0]),
            _ => {
                log::warn!(
                    "content contains more than one video stream; selecting stream with lowest index"
                );
                all_video_streams.sort_by_key(|v| v.stream_index);
                Ok(all_video_streams[0])
            }
        }
    }

    pub fn select_audio_stream(&self) -> Result<&ProbeResultAudioStream, FFPipelineError> {
        let mut all_audio_streams: Vec<&ProbeResultAudioStream> = self
            .audio_input
            .probe_result
            .streams
            .iter()
            .filter_map(|s| match s {
                ProbeResultStream::Audio(audio_stream) => Some(audio_stream),
                _ => None,
            })
            .collect();

        if let Some(audio_index) = self.audio_input.stream_index {
            let matched_stream = all_audio_streams
                .iter()
                .find(|a| a.stream_index == audio_index);

            match matched_stream {
                Some(audio_stream) => {
                    return Ok(audio_stream);
                }
                None => {
                    log::warn!(
                        "unable to locate requested audio stream with index {}",
                        audio_index
                    );
                }
            }
        }

        match all_audio_streams.len() {
            0 => Err(FFPipelineError::AudioInputIsRequired),
            1 => Ok(all_audio_streams[0]),
            _ => {
                log::warn!(
                    "content contains more than one audio stream; selecting stream with greatest number of channels"
                );
                all_audio_streams.sort_by_key(|a| std::cmp::Reverse(a.channels));
                Ok(all_audio_streams[0])
            }
        }
    }

    pub fn select_subtitle_stream(&self) -> Option<&ProbeResultVideoStream> {
        let all_subtitle_streams: Vec<&Box<ProbeResultVideoStream>> =
            match self.subtitle_input.as_ref() {
                Some(input) => input
                    .probe_result
                    .streams
                    .iter()
                    .filter_map(|s| match s {
                        ProbeResultStream::Video(video_stream)
                            if video_stream.codec_type == CodecType::Subtitle =>
                        {
                            Some(video_stream)
                        }
                        _ => None,
                    })
                    .collect(),
                None => Vec::new(),
            };

        if let Some(subtitle_index) = self.subtitle_input.as_ref().and_then(|i| i.stream_index) {
            let matched_stream = all_subtitle_streams
                .iter()
                .find(|a| a.stream_index == subtitle_index);

            match matched_stream {
                Some(subtitle_stream) => return Some(subtitle_stream),
                None => {
                    log::warn!(
                        "unable to locate requested subtitle stream with index {}",
                        subtitle_index
                    );
                }
            }
        }

        // at this point, select a subtitle if the input is *only* a subtitle
        if all_subtitle_streams.len() == 1
            && self
                .subtitle_input
                .as_ref()
                .map(|i| i.probe_result.streams.len() == 1)
                .unwrap_or(false)
        {
            Some(all_subtitle_streams[0])
        } else {
            None
        }
    }

    pub fn select_watermark_stream(&self) -> Option<&ProbeResultVideoStream> {
        let mut all_watermark_streams: Vec<&Box<ProbeResultVideoStream>> =
            match self.watermark_input.as_ref() {
                Some(input) => input
                    .probe_result
                    .streams
                    .iter()
                    .filter_map(|s| match s {
                        ProbeResultStream::Video(video_stream)
                            if video_stream.codec_type == CodecType::Video =>
                        {
                            Some(video_stream)
                        }
                        _ => None,
                    })
                    .collect(),
                None => Vec::new(),
            };

        if let Some(watermark_index) = self.watermark_input.as_ref().and_then(|i| i.stream_index) {
            let matched_stream = all_watermark_streams
                .iter()
                .find(|a| a.stream_index == watermark_index);

            match matched_stream {
                Some(watermark_stream) => return Some(watermark_stream),
                None => {
                    log::warn!(
                        "unable to locate requested watermark stream with index {}",
                        watermark_index
                    );
                }
            }
        }

        match all_watermark_streams.len() {
            0 => None,
            1 => Some(all_watermark_streams[0]),
            _ => {
                log::warn!(
                    "content contains more than one watermark video stream; selecting stream with lowest index"
                );
                all_watermark_streams.sort_by_key(|v| v.stream_index);
                Some(all_watermark_streams[0])
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct HttpInputOptions {
    pub headers: Vec<String>,
    pub user_agent: Option<String>,
    pub timeout_us: Option<u64>,
    pub reconnect: bool,
    pub reconnect_delay_max: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct LocalInputSource {
    pub path: String,
}

impl LocalInputSource {
    pub fn expand_path(&self) -> Option<String> {
        let expanded_path_buf = expand_tilde(self.path.as_str()); //.ok_or(ChannelError::PlayoutJsonInvalidLocalSource)?;
        expanded_path_buf
            .map(|p| p.into_os_string())
            .and_then(|p| p.into_string().ok())
    }
}

#[derive(Debug, Clone)]
pub struct LavfiInputSource {
    pub params: String,
}

#[derive(Debug, Clone)]
pub struct HttpInputSource {
    pub uri: String,
    pub options: HttpInputOptions,
}

#[derive(Debug, Clone)]
#[enum_dispatch(Probeable)]
#[enum_dispatch(FfmpegInputArgs)]
pub enum InputSource {
    Local(LocalInputSource),
    Lavfi(LavfiInputSource),
    Http(HttpInputSource),
}

#[enum_dispatch]
pub trait FfmpegInputArgs {
    fn args_for_input(&self) -> ArgVec;
}

impl FfmpegInputArgs for LocalInputSource {
    fn args_for_input(&self) -> ArgVec {
        vec![]
    }
}

impl FfmpegInputArgs for LavfiInputSource {
    fn args_for_input(&self) -> ArgVec {
        args!["-f", "lavfi"]
    }
}
impl FfmpegInputArgs for HttpInputSource {
    fn args_for_input(&self) -> ArgVec {
        let mut args: ArgVec = Vec::new();

        if self.options.reconnect {
            args.extend(args![
                "-reconnect",
                "1",
                "-reconnect_on_network_error",
                "1",
                "-reconnect_streamed",
                "1",
                "-multiple_requests",
                "1",
            ]);
            if let Some(max_delay) = self.options.reconnect_delay_max {
                args.extend(args!["-reconnect_delay_max", max_delay.to_string()]);
            }
        }

        if let Some(timeout) = self.options.timeout_us {
            args.extend(args!["-timeout", timeout.to_string()]);
        }

        if let Some(ua) = &self.options.user_agent {
            args.extend(args!["-user_agent", ua.clone()]);
        }

        if !self.options.headers.is_empty() {
            // FFmpeg expects headers separated by \r\n, with trailing \r\n
            let combined: String = self
                .options
                .headers
                .iter()
                .map(|h| format!("{}\r\n", h))
                .collect();
            args.extend(args!["-headers", combined]);
        }

        args.extend(args![
            "-protocol_whitelist",
            "file,http,https,tcp,tls,crypto",
        ]);

        args
    }
}

pub struct ProbedInput {
    pub input_source: InputSource,
    pub probe_result: ProbeResult,
    pub in_point: Duration,
    pub out_point: Duration,
    pub stream_index: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct WatermarkInput {
    pub input_source: InputSource,
    pub probe_result: ProbeResult,
    pub stream_index: Option<u32>,
    pub location: WatermarkLocation,
    pub width_percent: Option<f32>,
    pub horizontal_margin_percent: Option<f32>,
    pub vertical_margin_percent: Option<f32>,
    pub opacity_percent: Option<f32>,
}

impl WatermarkInput {
    pub(crate) fn scaled_size(
        &self,
        watermark_size: FrameSize,
        video_size: Option<FrameSize>,
    ) -> FrameSize {
        if let Some(output_size) = video_size
            && let Some(width_percent) = self.width_percent
        {
            let mut scaled_width =
                f32::round((width_percent / 100f32) * output_size.width as f32) as u32;
            let aspect_ratio = watermark_size.height as f32 / watermark_size.width as f32;
            let mut scaled_height = f32::round(scaled_width as f32 * aspect_ratio) as u32;
            if scaled_width % 2 == 1 {
                scaled_width += 1;
            }
            if scaled_height % 2 == 1 {
                scaled_height += 1;
            }
            FrameSize {
                width: scaled_width,
                height: scaled_height,
            }
        } else {
            watermark_size
        }
    }

    pub(crate) fn frame_location(
        &self,
        scaled_size: &FrameSize,
        video_size: &FrameSize,
    ) -> FramePoint {
        let horizontal_margin = f32::round(
            self.horizontal_margin_percent.unwrap_or(0f32) / 100f32 * video_size.width as f32,
        ) as u32;
        let vertical_margin = f32::round(
            self.vertical_margin_percent.unwrap_or(0f32) / 100f32 * video_size.height as f32,
        ) as u32;

        match self.location {
            WatermarkLocation::TopLeft => FramePoint {
                x: horizontal_margin,
                y: vertical_margin,
            },
            WatermarkLocation::TopCenter => FramePoint {
                x: video_size.width.saturating_sub(scaled_size.width) / 2 + horizontal_margin,
                y: vertical_margin,
            },
            WatermarkLocation::TopRight => FramePoint {
                x: video_size
                    .width
                    .saturating_sub(scaled_size.width)
                    .saturating_sub(horizontal_margin),
                y: vertical_margin,
            },
            WatermarkLocation::CenterLeft => FramePoint {
                x: horizontal_margin,
                y: video_size.height.saturating_sub(scaled_size.height) / 2 + vertical_margin,
            },
            WatermarkLocation::Center => FramePoint {
                x: video_size.width.saturating_sub(scaled_size.width) / 2 + horizontal_margin,
                y: video_size.height.saturating_sub(scaled_size.height) / 2 + vertical_margin,
            },
            WatermarkLocation::CenterRight => FramePoint {
                x: video_size
                    .width
                    .saturating_sub(scaled_size.width)
                    .saturating_sub(horizontal_margin),
                y: video_size.height.saturating_sub(scaled_size.height) / 2 + vertical_margin,
            },
            WatermarkLocation::BottomLeft => FramePoint {
                x: horizontal_margin,
                y: video_size
                    .height
                    .saturating_sub(scaled_size.height)
                    .saturating_sub(vertical_margin),
            },
            WatermarkLocation::BottomCenter => FramePoint {
                x: video_size.width.saturating_sub(scaled_size.width) / 2 + horizontal_margin,
                y: video_size
                    .height
                    .saturating_sub(scaled_size.height)
                    .saturating_sub(vertical_margin),
            },
            WatermarkLocation::BottomRight => FramePoint {
                x: video_size
                    .width
                    .saturating_sub(scaled_size.width)
                    .saturating_sub(horizontal_margin),
                y: video_size
                    .height
                    .saturating_sub(scaled_size.height)
                    .saturating_sub(vertical_margin),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum WatermarkLocation {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}
