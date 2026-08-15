use std::fmt::Formatter;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use strum::{Display, EnumString};

use crate::ArgVec;
use crate::audio_codec::AudioCodec;
use crate::audio_decoder::AudioDecoder;
use crate::audio_filter::AudioFilter;
use crate::error::FFPipelineError;
use crate::ffmpeg_info::FfmpegInfo;
use crate::filter_chain::{FilterChain, PipelineFilter};
use crate::frame_rate::FrameRate;
use crate::frame_size::FrameSize;
use crate::global_option::{GlobalOption, LogLevel};
use crate::hw_accel::{HardwareAccel, HwAccel};
use crate::input::{FfmpegInputArgs, GraphicsInput, InputSettings, InputSource};
use crate::output_option::OutputOption;
use crate::output_settings::{
    OutputSettings, ScalingMode, SubtitleMode, VideoFilterOptions, YadifOptions,
};
use crate::overlay_filter::{OverlayFilter, OverlaySource, SoftwareOverlay};
use crate::video_codec::VideoCodec;
use crate::video_decoder::VideoDecoder;
use crate::video_filter::{
    ColorChannelMixerFilter, CropFilter, DeinterlaceFilter, Dv5WorkaroundFilter, FadeFilter,
    FormatFilter, LoopFilter, PadFilter, ScaleFilter, SoftwareDeinterlaceFilter,
    SoftwareDeinterlaceOptions, SubtitleImageScaleFilter, SubtitlesFilter, TPadFilter,
    ToneMapFilter, VideoFilter,
};

pub const KEYFRAME_INTERVAL_SECONDS: u32 = 2;
pub const SEGMENT_SECONDS: u32 = 4;

#[derive(Debug, Clone, Copy, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum AudioFormat {
    Aac,
    Ac3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kbps(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hz(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize)]
#[strum(serialize_all = "lowercase")]
pub enum VideoFormat {
    Av1,
    H264,
    Hevc,
    Mpeg2Video,
    Vc1,
    Vp8,
    Vp9,
}

#[derive(Debug, Copy, Clone)]
pub struct PtsOffset {
    pub duration: Duration,
}

impl Default for PtsOffset {
    fn default() -> Self {
        PtsOffset {
            duration: Duration::ZERO,
        }
    }
}

pub(crate) struct OutputContext {
    pub(crate) media_frame_rate: FrameRate,
    pub(crate) audio_codec: AudioCodec,
    pub(crate) audio_channels: Option<u32>,
    pub(crate) video_codec: VideoCodec,
    pub(crate) pts_offset: Option<PtsOffset>,
    pub(crate) preferred_surface: FrameSurface,
    pub(crate) preferred_pixel_format: Option<PixelFormat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display)]
pub enum FrameSurface {
    System,
    Amf,
    Cuda,
    Qsv,
    Rkmpp,
    Vaapi,
    VideoToolbox,
    Vulkan,
    OpenCL,
}

impl FrameSurface {
    pub(crate) fn device_name(&self) -> Option<&'static str> {
        match self {
            FrameSurface::Amf => Some("amf"),
            FrameSurface::Cuda => Some("cuda"),
            FrameSurface::OpenCL => Some("opencl"),
            FrameSurface::Qsv => Some("qsv"),
            FrameSurface::Rkmpp => Some("rkmpp"),
            FrameSurface::Vaapi => Some("vaapi"),
            FrameSurface::Vulkan => Some("vulkan"),
            FrameSurface::VideoToolbox => Some("videotoolbox"),
            FrameSurface::System => None,
        }
    }
}

pub type SurfaceSet = std::collections::HashSet<FrameSurface>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Bgra,
    Yuv420p,
    Yuv420p10le,
    Yuva420p,
    Yuva420p10le,
    Nv12,
    Nv15,
    P010le,
    P016,
}

gen_subset!(HwPixelFormat, PixelFormat, Nv12, Nv15, P010le);

impl PixelFormat {
    pub(crate) fn parse(pix_fmt: &str) -> PixelFormat {
        match pix_fmt.to_lowercase().as_str() {
            "bgra" => PixelFormat::Bgra,
            "yuv420p" => PixelFormat::Yuv420p,
            "yuv420p10le" => PixelFormat::Yuv420p10le,
            "yuva420p" => PixelFormat::Yuva420p,
            "yuva420p10le" => PixelFormat::Yuva420p10le,
            "nv12" => PixelFormat::Nv12,
            "nv15" => PixelFormat::Nv15,
            "p010le" => PixelFormat::P010le,
            _ => {
                log::warn!("assuming unknown pixel format {} is yuv420p", pix_fmt);
                PixelFormat::Yuv420p
            }
        }
    }

    pub(crate) fn bit_depth(&self) -> u8 {
        match self {
            PixelFormat::Bgra
            | PixelFormat::Yuv420p
            | PixelFormat::Yuva420p
            | PixelFormat::Nv12 => 8,
            PixelFormat::Yuv420p10le
            | PixelFormat::Yuva420p10le
            | PixelFormat::P010le
            | PixelFormat::Nv15 => 10,
            PixelFormat::P016 => 16,
        }
    }

    pub(crate) fn has_alpha(&self) -> bool {
        matches!(
            self,
            PixelFormat::Bgra | PixelFormat::Yuva420p | PixelFormat::Yuva420p10le
        )
    }

    pub(crate) fn as_arg(&self) -> &str {
        match self {
            PixelFormat::Bgra => "bgra",
            PixelFormat::Yuv420p => "yuv420p",
            PixelFormat::Yuv420p10le => "yuv420p10le",
            PixelFormat::Yuva420p => "yuva420p",
            PixelFormat::Yuva420p10le => "yuva420p10le",
            PixelFormat::Nv12 => "nv12",
            PixelFormat::Nv15 => "nv15",
            PixelFormat::P010le => "p010le",
            PixelFormat::P016 => "p016",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HdrFormat {
    None,
    Pq,
    Hlg,
    Dv5,
}

#[derive(Clone, Debug, derive_more::Display)]
#[display(
    "FrameState(size={},is_anamorphic={},surface={})",
    size,
    is_anamorphic,
    surface
)]
pub struct FrameState {
    pub(crate) size: FrameSize,
    pub(crate) is_anamorphic: bool,
    pub(crate) is_interlaced: bool,
    pub(crate) sample_aspect_ratio: Option<String>,
    pub(crate) display_aspect_ratio: Option<String>,
    pub(crate) surface: FrameSurface,
    pub(crate) pixel_format: PixelFormat,
    pub(crate) hdr_format: HdrFormat,
}

pub enum PipelineInput {
    Audio {
        input_source: InputSource,
        index: u32,
        path: String,
        seek: Duration,
        channels: u32,
        decoder: AudioDecoder,
    },
    Video {
        input_source: InputSource,
        index: u32,
        path: String,
        seek: Duration,
        realtime: bool,
        decoder: VideoDecoder,
    },
    Subtitle {
        input_source: InputSource,
        index: u32,
        path: String,
        seek: Duration,
    },
    Graphics {
        input: GraphicsInput,
        layer_index: usize,
        index: u32,
        path: String,
        extra_input_args: ArgVec,
    },
}

impl PipelineInput {
    fn sort_order(&self) -> usize {
        match self {
            PipelineInput::Video { .. } => 0,
            PipelineInput::Audio { .. } => 1,
            PipelineInput::Subtitle { .. } => 2,
            PipelineInput::Graphics { layer_index, .. } => 3 + *layer_index,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnvironmentVariable {
    pub key: String,
    pub value: String,
}

pub struct Pipeline {
    ffmpeg_info: FfmpegInfo,
    accel: Option<HardwareAccel>,
    filter_options: VideoFilterOptions,
    initial_state: FrameState,

    global_options: Vec<GlobalOption>,
    inputs: Vec<PipelineInput>,
    filter_chain: FilterChain,
    output_options: Vec<OutputOption>,
    env_vars: Vec<EnvironmentVariable>,

    output_context: OutputContext,
}

impl Pipeline {
    fn full(
        ffmpeg_info: &FfmpegInfo,
        input_settings: InputSettings,
        output_settings: OutputSettings,
    ) -> Result<Pipeline, FFPipelineError> {
        let mut final_output_settings = output_settings;

        if let Some(accel) = &final_output_settings.accel
            && accel
                .known_accel()
                .map(|a| !ffmpeg_info.has_hw_accel(a))
                .unwrap_or(false)
        {
            log::warn!("ffmpeg does not support requested accel {:?}", accel);
            final_output_settings.accel = None;
        }

        let duration = std::cmp::min(
            input_settings.audio_input.out_point - input_settings.audio_input.in_point,
            input_settings.video_input.out_point - input_settings.video_input.in_point,
        );

        let audio_codec = match final_output_settings.audio.format {
            Some(AudioFormat::Aac) => AudioCodec::Aac,
            Some(AudioFormat::Ac3) => AudioCodec::Ac3,
            _ => AudioCodec::Copy,
        };

        let video_stream = input_settings.select_video_stream()?;
        let audio_stream = input_settings.select_audio_stream()?;
        let subtitle_stream = input_settings.select_subtitle_stream();
        let graphics_streams: Vec<_> = input_settings
            .graphics_inputs
            .iter()
            .map(|input| input_settings.select_graphics_stream(input))
            .collect();

        // TODO: add target profile to config
        let video_codec = match (
            final_output_settings.accel.as_ref(),
            final_output_settings.video_format,
        ) {
            (Some(a), Some(format)) => a
                .codec_for_format(
                    &format,
                    final_output_settings.bit_depth.unwrap_or(8),
                    final_output_settings.video_size,
                )
                .filter(|_| a.can_encode(&format, final_output_settings.bit_depth.unwrap_or(8)))
                .unwrap_or(match format {
                    VideoFormat::Hevc => VideoCodec::libx265(),
                    VideoFormat::H264 => VideoCodec::libx264(),
                    _ => VideoCodec::copy(),
                }),
            (_, Some(VideoFormat::H264)) => VideoCodec::libx264(),
            (_, Some(VideoFormat::Hevc)) => VideoCodec::libx265(),
            _ => VideoCodec::copy(),
        };

        let is_still_image = input_settings.video_input.probe_result.is_still_image();
        let video_decoder = VideoDecoder::new(
            ffmpeg_info,
            video_stream,
            is_still_image,
            &final_output_settings,
        );

        let hdr = match (
            video_stream.dv_profile,
            video_stream.color_params.color_transfer.as_deref(),
        ) {
            (Some(5), _) => HdrFormat::Dv5,
            (_, Some("smpte2084")) => HdrFormat::Pq,
            (_, Some("arib-std-b67")) => HdrFormat::Hlg,
            _ => HdrFormat::None,
        };

        let initial_state = FrameState {
            size: FrameSize {
                width: video_stream
                    .width
                    .ok_or(FFPipelineError::VideoInputIsRequired)?,
                height: video_stream
                    .height
                    .ok_or(FFPipelineError::VideoInputIsRequired)?,
            },
            is_anamorphic: video_stream.is_anamorphic(),
            // if user does not want to deinterlace, pretend content is not interlaced
            is_interlaced: final_output_settings.deinterlace && video_stream.is_interlaced(),
            sample_aspect_ratio: video_stream.sample_aspect_ratio.to_owned(),
            display_aspect_ratio: video_stream.display_aspect_ratio.to_owned(),
            surface: video_decoder.output_surface(),
            pixel_format: video_decoder
                .output_format(&PixelFormat::parse(video_stream.pix_fmt.as_str())),
            hdr_format: hdr,
        };

        let preferred_pixel_format = match final_output_settings.bit_depth {
            Some(10) => video_codec.preferred_pixel_format_10bit,
            Some(8) => video_codec.preferred_pixel_format_8bit,
            _ => None,
        };

        let output_context = OutputContext {
            audio_codec,
            audio_channels: final_output_settings.audio.channels,
            video_codec: video_codec.clone(),
            pts_offset: final_output_settings.pts_offset,
            media_frame_rate: video_stream.frame_rate.to_owned(),
            preferred_surface: video_codec.preferred_surface,
            preferred_pixel_format,
        };

        let mut filters = vec![
            PipelineFilter::Audio(AudioFilter::LoudNorm {
                settings: final_output_settings.audio.loudness.clone(),
                sample_rate: final_output_settings.audio.sample_rate,
            }),
            PipelineFilter::Audio(AudioFilter::Resample),
            PipelineFilter::Audio(AudioFilter::Pad),
        ];

        if final_output_settings.pad_to_duration {
            // pad an under-running source to the output -t clamp by cloning
            // its last frame, so the item still fills its scheduled duration
            // (audio is always padded; see AudioFilter::Pad)
            filters.push(PipelineFilter::Video(TPadFilter.into()));
        }

        filters.extend([
            PipelineFilter::Video(LoopFilter { is_still_image }.into()),
            PipelineFilter::Video(Dv5WorkaroundFilter.into()),
            PipelineFilter::Video(
                ToneMapFilter {
                    algorithm: final_output_settings.filter_options.tonemap.tonemap.clone(),
                    output_format: match final_output_settings.bit_depth {
                        Some(10) => PixelFormat::Yuv420p10le,
                        _ => PixelFormat::Yuv420p,
                    },
                }
                .into(),
            ),
            PipelineFilter::Video(
                DeinterlaceFilter {
                    filter: SoftwareDeinterlaceFilter::Yadif(YadifOptions::default()),
                    options: SoftwareDeinterlaceOptions {
                        bwdif: final_output_settings.filter_options.bwdif.clone(),
                        w3fdif: final_output_settings.filter_options.w3fdif.clone(),
                        yadif: final_output_settings.filter_options.yadif.clone(),
                    },
                    input_is_interlaced: initial_state.is_interlaced,
                }
                .into(),
            ),
            PipelineFilter::Video(
                ScaleFilter {
                    size: final_output_settings.video_size,
                    scaling_mode: final_output_settings.scaling_mode,
                    input_is_anamorphic: initial_state.is_anamorphic,
                    force_original_aspect_ratio: None,
                }
                .into(),
            ),
            PipelineFilter::Video(
                PadFilter {
                    size: final_output_settings.video_size.to_owned(),
                    scaling_mode: final_output_settings.scaling_mode,
                }
                .into(),
            ),
            PipelineFilter::Video(
                CropFilter {
                    size: final_output_settings.video_size.to_owned(),
                    scaling_mode: final_output_settings.scaling_mode,
                }
                .into(),
            ),
        ]);

        let mut inputs = vec![
            PipelineInput::Audio {
                input_source: input_settings.audio_input.input_source.to_owned(),
                index: audio_stream.stream_index,
                path: input_settings.audio_input.probe_result.path.to_owned(),
                seek: input_settings.audio_input.in_point,
                channels: audio_stream.channels,
                decoder: AudioDecoder::new(audio_stream, &final_output_settings),
            },
            PipelineInput::Video {
                input_source: input_settings.video_input.input_source.to_owned(),
                index: video_stream.stream_index,
                path: input_settings.video_input.probe_result.path.to_owned(),
                seek: if is_still_image {
                    Duration::ZERO
                } else {
                    input_settings.video_input.in_point
                },
                realtime: final_output_settings.realtime && !final_output_settings.is_live,
                decoder: video_decoder,
            },
        ];

        if let Some(subtitle_stream) = subtitle_stream
            && let Some(subtitle_input) = input_settings.subtitle_input.as_ref()
        {
            if subtitle_stream.is_subtitle_image()
                && let Some(size) = final_output_settings.video_size
            {
                inputs.push(PipelineInput::Subtitle {
                    input_source: subtitle_input.input_source.to_owned(),
                    index: subtitle_stream.stream_index,
                    path: subtitle_input.probe_result.path.to_owned(),
                    seek: subtitle_input.in_point,
                });

                let secondary_initial_state = FrameState {
                    size,
                    is_anamorphic: subtitle_stream.is_anamorphic(),
                    is_interlaced: false,
                    sample_aspect_ratio: subtitle_stream.sample_aspect_ratio.to_owned(),
                    display_aspect_ratio: subtitle_stream.display_aspect_ratio.to_owned(),
                    surface: FrameSurface::System,
                    pixel_format: if subtitle_stream.pix_fmt.is_empty() {
                        PixelFormat::Bgra
                    } else {
                        PixelFormat::parse(&subtitle_stream.pix_fmt)
                    },
                    hdr_format: HdrFormat::None,
                };

                filters.push(PipelineFilter::Overlay(OverlayFilter {
                    kind: SoftwareOverlay::default().into(),
                    secondary: vec![SubtitleImageScaleFilter { size }.into()],
                    secondary_initial_state,
                    secondary_source: OverlaySource::Subtitle,
                    location: None,
                }));
            } else if !subtitle_stream.is_subtitle_image()
                && final_output_settings.subtitle_mode == SubtitleMode::Burn
            {
                // only use force_style with SRT, which doesn't have any styling of its own
                let mut final_force_style = None;
                if subtitle_stream.codec == "srt" || subtitle_stream.codec == "subrip" {
                    final_force_style = final_output_settings.subtitle_force_style;
                }

                filters.push(PipelineFilter::Video(
                    SubtitlesFilter {
                        path: subtitle_input.probe_result.path.to_owned(),
                        seek: subtitle_input.in_point,
                        fonts_folder: final_output_settings.fonts_folder.to_owned(),
                        force_style: final_force_style,
                    }
                    .into(),
                ))
            }
        }

        for (graphics_input, graphics_stream) in
            input_settings.graphics_inputs.iter().zip(graphics_streams)
        {
            let Some(graphics_stream) = graphics_stream else {
                return Err(FFPipelineError::GraphicsStreamNotFound(
                    graphics_input.layer_index,
                ));
            };
            let (Some(height), Some(width)) = (graphics_stream.height, graphics_stream.width)
            else {
                return Err(FFPipelineError::GraphicsStreamNotFound(
                    graphics_input.layer_index,
                ));
            };
            let extra_input_args = if graphics_stream.is_still_image() {
                // decode a single frame; the loop filter below repeats it *after* scaling, so
                // decode and scale happen once instead of once per output frame
                args![
                    "-framerate",
                    output_context.media_frame_rate.r_frame_rate.clone()
                ]
            } else if graphics_stream.codec == "gif" || graphics_stream.codec == "apng" {
                args![
                    "-ignore_loop",
                    "0",
                    "-t",
                    format!("{}ms", duration.as_millis())
                ]
            } else {
                args![
                    "-stream_loop",
                    "-1",
                    "-t",
                    format!("{}ms", duration.as_millis())
                ]
            };

            inputs.push(PipelineInput::Graphics {
                input: graphics_input.clone(),
                layer_index: graphics_input.layer_index,
                index: graphics_stream.stream_index,
                path: graphics_input.probe_result.path.to_owned(),
                extra_input_args,
            });

            let secondary_initial_state = FrameState {
                size: FrameSize { width, height },
                is_anamorphic: false,
                is_interlaced: false,
                sample_aspect_ratio: Some(String::from("1:1")),
                display_aspect_ratio: None,
                surface: FrameSurface::System,
                pixel_format: if graphics_stream.pix_fmt.is_empty() {
                    PixelFormat::Bgra
                } else {
                    PixelFormat::parse(&graphics_stream.pix_fmt)
                },
                hdr_format: HdrFormat::None,
            };

            let video_size = final_output_settings
                .video_size
                .as_ref()
                .unwrap_or(&initial_state.size);

            let source_content_size = match final_output_settings.scaling_mode {
                ScalingMode::ScaleAndPad => video_size.square_pixel_size_contain(&initial_state),
                ScalingMode::Crop | ScalingMode::Stretch => *video_size,
            };

            let scaled_size = graphics_input.scaled_size(
                FrameSize { width, height },
                final_output_settings.video_size,
            );

            let location =
                Some(graphics_input.frame_location(&source_content_size, &scaled_size, video_size));

            let fade_filters = FadeFilter::for_graphics(
                graphics_input.timing.as_ref(),
                input_settings.start,
                input_settings.playout_offset,
                duration,
            );

            let mut secondary_filters: Vec<VideoFilter> = vec![
                ColorChannelMixerFilter {
                    alpha: graphics_input.opacity_percent.unwrap_or(100f32) / 100.0f32,
                }
                .into(),
                FormatFilter {
                    format: match secondary_initial_state.pixel_format.bit_depth() {
                        10 => PixelFormat::Yuva420p10le,
                        _ => PixelFormat::Yuva420p,
                    },
                }
                .into(),
                ScaleFilter {
                    size: Some(scaled_size),
                    scaling_mode: ScalingMode::ScaleAndPad,
                    input_is_anamorphic: false,
                    force_original_aspect_ratio: None,
                }
                .into(),
            ];

            // a still image is decoded as a single frame; only fades need it repeated (they act on
            // frame timestamps). otherwise the overlay's repeatlast holds it, which keeps the
            // per-frame format conversion and hwupload out of the chain entirely
            if !fade_filters.is_empty() {
                secondary_filters.push(
                    LoopFilter {
                        is_still_image: graphics_stream.is_still_image(),
                    }
                    .into(),
                );
            }

            secondary_filters.extend(fade_filters.iter().map(|f| f.clone().into()));

            filters.push(PipelineFilter::Overlay(OverlayFilter {
                kind: SoftwareOverlay::default().into(),
                secondary: secondary_filters,
                secondary_initial_state,
                secondary_source: OverlaySource::Graphics(graphics_input.layer_index),
                location,
            }));
        }

        let mut env_vars = Vec::new();

        if let Some(reports_folder) = final_output_settings
            .reports_folder
            .as_deref()
            .filter(|s| !s.is_empty())
            && let Some(report_id) = final_output_settings
                .report_id
                .as_deref()
                .filter(|s| !s.is_empty())
        {
            let folder = PathBuf::from(reports_folder);
            if let Err(err) = std::fs::create_dir_all(&folder) {
                log::warn!("failed to create ffmpeg reports folder: {err}; will not save report");
            } else {
                let file = folder
                    .join(format!(".in-flight-{}.log", report_id))
                    .to_string_lossy()
                    .to_string()
                    .replace(r"%", r"%%");

                #[cfg(target_os = "windows")]
                let mut file = file;

                #[cfg(target_os = "windows")]
                {
                    file = file.replace(r"\", r"/").replace(r":/", r"\:/");
                }

                env_vars = vec![EnvironmentVariable {
                    key: String::from("FFREPORT"),
                    value: format!("file={file}:level=32"),
                }]
            }
        }

        Ok(Pipeline {
            ffmpeg_info: ffmpeg_info.clone(),
            accel: final_output_settings.accel.clone(),
            filter_options: final_output_settings.filter_options,
            initial_state: initial_state.clone(),
            global_options: vec![
                // hardware accel should use a single thread
                GlobalOption::Threads(match &final_output_settings.accel {
                    Some(_) => 1,
                    _ => 0,
                }),
                GlobalOption::NoStdIn,
                GlobalOption::HideBanner,
                GlobalOption::LogLevel(LogLevel::Error),
                GlobalOption::StandardFormatFlags,
            ],
            inputs,
            filter_chain: FilterChain::new(filters),
            output_options: vec![
                OutputOption::NoDemuxDecodeDelay,
                OutputOption::MovFlagsFastStart,
                OutputOption::CudaNoAutoScale,
                OutputOption::AudioCodec(audio_codec),
                OutputOption::AudioBitrate(final_output_settings.audio.bitrate),
                OutputOption::AudioBuffer(final_output_settings.audio.buffer),
                OutputOption::AudioChannels(final_output_settings.audio.channels),
                OutputOption::AudioSampleRate(final_output_settings.audio.sample_rate),
                OutputOption::VideoCodec(video_codec),
                OutputOption::VideoBitrate(final_output_settings.video_bitrate),
                OutputOption::VideoBuffer(final_output_settings.video_buffer),
                OutputOption::DoNotMapMetadata,
                OutputOption::Duration(duration),
                OutputOption::TsOffset(final_output_settings.pts_offset),
                OutputOption::VideoTrackTimeScale(90_000),
                OutputOption::FrameRate(final_output_settings.frame_rate.clone()),
                OutputOption::Format(final_output_settings.format),
            ],
            output_context,
            env_vars,
        })
    }

    pub fn optimize(&mut self) {
        // audio copy shouldn't have bitrate etc
        if self.output_context.audio_codec == AudioCodec::Copy {
            self.output_options.retain(|o| {
                !matches!(
                    o,
                    OutputOption::AudioBitrate(_)
                        | OutputOption::AudioBuffer(_)
                        | OutputOption::AudioChannels(_)
                        | OutputOption::AudioSampleRate(_)
                )
            });

            self.filter_chain.disable_audio();
        };

        // remove audio channels output option if input channel count matches
        if let Some(audio_channels) = self.inputs.iter().find_map(|s| match s {
            PipelineInput::Audio { channels, .. } => Some(channels),
            _ => None,
        }) && Some(audio_channels) == self.output_context.audio_channels.as_ref()
        {
            self.output_options
                .retain(|o| !matches!(o, OutputOption::AudioChannels(_)));
        }

        // video copy shouldn't have bitrate, etc
        if self.output_context.video_codec.codec_name == VideoCodec::COPY {
            self.output_options.retain(|o| {
                !matches!(
                    o,
                    OutputOption::VideoBitrate(_) | OutputOption::VideoBuffer(_)
                )
            });

            self.filter_chain.disable_video();
        }

        self.filter_chain
            .evaluate(&self.initial_state, &self.ffmpeg_info);
        self.filter_chain.resolve(
            &self.ffmpeg_info,
            &self.accel,
            &self.filter_options,
            &self.initial_state,
            &self.output_context.preferred_surface,
            &self.output_context.preferred_pixel_format,
        );

        // prepend decoder filters;
        // this is a special case that's only really needed for CUDA's hwupload workaround
        if let Some(video_decoder) = self.inputs.iter().find_map(|s| match s {
            PipelineInput::Video { decoder, .. } => Some(decoder),
            _ => None,
        }) {
            self.filter_chain.prepend(video_decoder.filters());
        }

        self.filter_chain.optimize();

        if let Some(accel) = &self.accel {
            let mut surfaces = self.filter_chain.surfaces().clone();
            surfaces.insert(self.initial_state.surface);
            surfaces.insert(self.output_context.preferred_surface);
            if surfaces.iter().any(|s| *s != FrameSurface::System) {
                let args = accel.init_hw_device(&surfaces);
                self.global_options.push(GlobalOption::InitHwDevice(args));
            }
        }
    }

    pub fn args(&self) -> ArgVec {
        let mut result: ArgVec = Vec::new();

        let mut audio_label = String::from("0:a");
        let mut video_label = String::from("0:v");
        let mut subtitle_label = None;
        let mut graphics_labels = vec![
            None;
            self.inputs
                .iter()
                .filter_map(|i| match i {
                    PipelineInput::Graphics { layer_index, .. } => Some(*layer_index),
                    _ => None,
                })
                .max()
                .map_or(0, |i| i + 1)
        ];

        let mut input_paths: Vec<&str> = Vec::new();

        let mut sorted_inputs: Vec<&PipelineInput> = self.inputs.iter().collect();
        sorted_inputs.sort_by_key(|i| i.sort_order());

        result.extend(self.global_options.iter().flat_map(|o| o.as_arg()));

        for input in sorted_inputs.iter() {
            match input {
                PipelineInput::Video {
                    input_source,
                    index,
                    path,
                    seek,
                    realtime,
                    decoder,
                    ..
                } => {
                    input_paths.push(path.as_str());

                    result.extend(decoder.as_arg());

                    let video_input_index = input_paths.iter().position(|p| p == path).unwrap_or(0);
                    video_label = format!("{}:{}", video_input_index, index);

                    if !seek.is_zero() {
                        result.extend(args!["-ss", format!("{}ms", seek.as_millis())]);
                    }

                    if *realtime {
                        result.extend(args!["-readrate", "1.0"]);
                    }

                    result.extend(input_source.args_for_input());
                    // TODO: if audio has same input and args, should use here

                    result.extend(args!["-i", path.to_owned()]);
                }
                PipelineInput::Audio {
                    input_source,
                    index,
                    path,
                    decoder,
                    ..
                } => {
                    // if we haven't yet used this input, add it
                    if !input_paths.contains(&path.as_str()) {
                        input_paths.push(path.as_str());

                        result.extend(decoder.as_arg());

                        // TODO: seek?

                        result.extend(input_source.args_for_input());
                        result.extend(args!["-i", path.to_owned()]);
                    }

                    let audio_input_index = input_paths.iter().position(|p| p == path).unwrap_or(0);
                    audio_label = format!("{}:{}", audio_input_index, index);
                }
                PipelineInput::Subtitle {
                    input_source,
                    index,
                    path,
                    seek,
                    ..
                } => {
                    if !input_paths.contains(&path.as_str()) {
                        input_paths.push(path.as_str());

                        if !seek.is_zero() {
                            result.extend(args!["-ss", format!("{}ms", seek.as_millis())]);
                        }

                        result.extend(input_source.args_for_input());
                        result.extend(args!["-i", path.to_owned()]);
                    }

                    let subtitle_input_index =
                        input_paths.iter().position(|p| p == path).unwrap_or(0);
                    subtitle_label = Some(format!("{}:{}", subtitle_input_index, index));
                }
                PipelineInput::Graphics {
                    input,
                    layer_index,
                    index,
                    path,
                    extra_input_args,
                } => {
                    input_paths.push(path.as_str());
                    result.extend(input.input_source.args_for_input());
                    result.extend(extra_input_args.clone());
                    result.extend(args!["-i", path.to_owned()]);
                    let graphics_input_index = input_paths.len() - 1;
                    graphics_labels[*layer_index] =
                        Some(format!("{}:{}", graphics_input_index, index));
                }
            }
        }

        let mut filter_chain = self.filter_chain.to_owned();
        filter_chain.build(
            &audio_label,
            &video_label,
            subtitle_label.as_ref(),
            &graphics_labels,
        );

        result.extend(filter_chain.as_arg());

        result.extend(args!["-map", filter_chain.video_label().to_owned()]);
        result.extend(args!["-map", filter_chain.audio_label().to_owned()]);

        result.extend(
            self.output_options
                .iter()
                .flat_map(|o| o.as_arg(&self.output_context)),
        );

        result
    }

    pub fn envs(&self) -> Vec<EnvironmentVariable> {
        let mut result = self.env_vars.clone();

        if let Some(a) = &self.accel {
            result.extend(a.envs())
        }

        result
    }
}

impl std::fmt::Display for Pipeline {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "args: {}", self.args().join(" "))
    }
}

pub fn generate_pipeline(
    ffmpeg_info: &FfmpegInfo,
    input_settings: InputSettings,
    output_settings: OutputSettings,
) -> Result<Pipeline, FFPipelineError> {
    Pipeline::full(ffmpeg_info, input_settings, output_settings)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use time::OffsetDateTime;

    use super::*;
    use crate::input::{LocalInputSource, ProbedInput};
    use crate::output_format::OutputFormat;
    use crate::output_settings::{
        AudioOutputSettings, OutputSettings, ScalingMode, SubtitleMode, VideoFilterOptions,
    };
    use crate::probe::{
        CodecType, ProbeResult, ProbeResultAudioStream, ProbeResultColorParams, ProbeResultStream,
        ProbeResultVideoStream,
    };

    #[test]
    fn device_name_returns_correct_ffmpeg_device_strings() {
        assert_eq!(FrameSurface::Cuda.device_name(), Some("cuda"));
        assert_eq!(FrameSurface::OpenCL.device_name(), Some("opencl"));
        assert_eq!(FrameSurface::Qsv.device_name(), Some("qsv"));
        assert_eq!(FrameSurface::Vaapi.device_name(), Some("vaapi"));
        assert_eq!(FrameSurface::Vulkan.device_name(), Some("vulkan"));
        assert_eq!(
            FrameSurface::VideoToolbox.device_name(),
            Some("videotoolbox")
        );
        assert_eq!(FrameSurface::System.device_name(), None);
    }

    fn file_probe(media: Duration) -> ProbeResult {
        ProbeResult {
            path: String::from("/media/item.mp4"),
            streams: vec![
                ProbeResultStream::Video(Box::new(ProbeResultVideoStream {
                    stream_index: 0,
                    codec: String::from("h264"),
                    codec_type: CodecType::Video,
                    profile: String::from("high"),
                    height: Some(1080),
                    width: Some(1920),
                    frame_rate: FrameRate::parse("30"),
                    sample_aspect_ratio: None,
                    display_aspect_ratio: None,
                    pix_fmt: String::from("yuv420p"),
                    color_params: ProbeResultColorParams::default(),
                    field_order: None,
                    dv_profile: None,
                })),
                ProbeResultStream::Audio(ProbeResultAudioStream {
                    stream_index: 1,
                    codec: String::from("aac"),
                    channels: 2,
                }),
            ],
            duration: Some(media),
            format_name: Some(String::from("mov,mp4,m4a")),
        }
    }

    /// A software pipeline for a plain file item, with or without
    /// `pad_to_duration`.
    fn file_pipeline_args(pad_to_duration: bool) -> ArgVec {
        let probe = file_probe(Duration::from_secs(10));
        let input_source = InputSource::Local(LocalInputSource {
            path: String::from("/media/item.mp4"),
        });

        let probed = |source: InputSource, probe: ProbeResult| ProbedInput {
            input_source: source,
            probe_result: probe,
            in_point: Duration::ZERO,
            out_point: Duration::from_millis(11_021),
            stream_index: None,
        };

        let input_settings = InputSettings {
            start: OffsetDateTime::UNIX_EPOCH,
            playout_offset: Duration::ZERO,
            audio_input: probed(input_source.clone(), probe.clone()),
            video_input: probed(input_source, probe),
            subtitle_input: None,
            graphics_inputs: Vec::new(),
        };

        let output_settings = OutputSettings {
            audio: AudioOutputSettings {
                format: Some(AudioFormat::Aac),
                bitrate: Some(Kbps(192)),
                buffer: Some(Kbps(384)),
                channels: Some(2),
                sample_rate: Some(Hz(48000)),
                loudness: None,
            },
            video_format: Some(VideoFormat::H264),
            bit_depth: Some(8),
            video_bitrate: Some(Kbps(5000)),
            video_buffer: Some(Kbps(10000)),
            video_size: None,
            scaling_mode: ScalingMode::ScaleAndPad,
            filter_options: VideoFilterOptions::default(),
            deinterlace: false,
            accel: None,
            format: OutputFormat::Hls {
                playlist: String::from("/session/live.m3u8"),
                segment_template: String::from("/session/live%05d.ts"),
                troubleshoot: false,
            },
            pts_offset: None,
            pad_to_duration,
            realtime: false,
            is_live: false,
            frame_rate: None,
            subtitle_mode: SubtitleMode::Burn,
            fonts_folder: None,
            subtitle_force_style: None,
            reports_folder: None,
            report_id: None,
        };

        let ffmpeg_info = FfmpegInfo {
            hwaccels: HashSet::new(),
            video_filters: HashSet::new(),
            preferred_filters: HashMap::new(),
        };

        let mut pipeline =
            generate_pipeline(&ffmpeg_info, input_settings, output_settings).unwrap();
        pipeline.optimize();
        pipeline.args()
    }

    /// This guards the wiring rather than the arithmetic: the flag has to
    /// reach the filter chain, and nothing else in these tests would notice
    /// if it stopped.
    #[test]
    fn padding_to_duration_puts_tpad_in_the_chain() {
        let has_tpad = |args: &ArgVec| args.iter().any(|a| a.as_ref().contains("tpad="));

        let padded = file_pipeline_args(true);
        let unpadded = file_pipeline_args(false);

        assert!(
            has_tpad(&padded),
            "a padded pipeline must carry tpad, got {padded:?}"
        );
        assert!(
            !has_tpad(&unpadded),
            "an unpadded pipeline must not, got {unpadded:?}"
        );
    }
}
