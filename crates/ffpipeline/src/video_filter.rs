use std::time::Duration;

use enum_dispatch::enum_dispatch;
use time::OffsetDateTime;

use crate::accel;
use crate::ffmpeg_info::{FfmpegInfo, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::input::{PeriodicClock, PeriodicTiming, WatermarkTiming};
use crate::output_settings::{BwdifOptions, ScalingMode, W3fdifOptions, YadifOptions};
use crate::pipeline::{FrameState, FrameSurface, HdrFormat, PixelFormat};

#[derive(Debug, Clone)]
pub enum ForceOriginalAspectRatio {
    Increase,
    Decrease,
}

impl ForceOriginalAspectRatio {
    pub(crate) fn as_arg(&self) -> String {
        match self {
            ForceOriginalAspectRatio::Increase => {
                String::from(":force_original_aspect_ratio=increase")
            }
            ForceOriginalAspectRatio::Decrease => {
                String::from(":force_original_aspect_ratio=decrease")
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SoftwareDeinterlaceOptions {
    pub bwdif: BwdifOptions,
    pub w3fdif: W3fdifOptions,
    pub yadif: YadifOptions,
}

#[derive(Debug, Clone)]
pub enum SoftwareDeinterlaceFilter {
    Bwdif(BwdifOptions),
    Yadif(YadifOptions),
    W3fdif(W3fdifOptions),
}

#[enum_dispatch]
pub trait VideoFilterOp {
    fn evaluate(&self, state: &FrameState, ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter>;
    fn apply_to(&self, state: &mut FrameState);
    fn required_surface(&self) -> Option<FrameSurface>;
    fn as_arg(&self) -> Option<String>;
}

#[derive(Debug, Clone)]
#[enum_dispatch(VideoFilterOp)]
pub enum VideoFilter {
    HwUpload(HwUploadFilter),
    HwDownload(HwDownloadFilter),
    Scale(ScaleFilter),
    Pad(PadFilter),
    Loop(LoopFilter),
    Format(FormatFilter),
    ToneMap(ToneMapFilter),
    Deinterlace(DeinterlaceFilter),
    HwMap(HwMapFilter),
    Subtitles(SubtitlesFilter),
    SubtitleImageScale(SubtitleImageScaleFilter),
    ColorChannelMixer(ColorChannelMixerFilter),
    Fade(FadeFilter),
    Crop(CropFilter),
    TPad(TPadFilter),
    Dv5Workaround(Dv5WorkaroundFilter),
    // CUDA hardware filters
    ScaleCuda(accel::cuda::ScaleCuda),
    PadCuda(accel::cuda::PadCuda),
    FormatCuda(accel::cuda::FormatCuda),
    HwUploadCudaWorkaround(accel::cuda::HwUploadCudaWorkaround),
    LibplaceboCuda(accel::cuda::LibplaceboCuda),
    DeinterlaceCuda(accel::cuda::DeinterlaceCuda),
    // VAAPI hardware filters
    DeinterlaceVaapi(accel::vaapi::DeinterlaceVaapi),
    ScaleVaapi(accel::vaapi::ScaleVaapi),
    PadVaapi(accel::vaapi::PadVaapi),
    FormatVaapi(accel::vaapi::FormatVaapi),
    TonemapVaapi(accel::vaapi::TonemapVaapi),
    // OpenCL hardware filters
    PadOpencl(accel::opencl::PadOpencl),
    TonemapOpencl(accel::opencl::TonemapOpencl),
    // RKRGA hardware filters
    FormatRkrga(accel::rkmpp::FormatRkrga),
    ScaleRkrga(accel::rkmpp::ScaleRkrga),
    // QSV hardware filters
    ScaleQsv(accel::qsv::ScaleQsv),
    FormatQsv(accel::qsv::FormatQsv),
    DeinterlaceQsv(accel::qsv::DeinterlaceQsv),
    // Vulkan hardware filters
    ScaleVulkan(accel::vulkan::ScaleVulkan),
    FormatVulkan(accel::vulkan::FormatVulkan),
    LibplaceboVulkan(accel::vulkan::LibplaceboVulkan),
    // VideoToolbox hardware filters
    ScaleVt(accel::video_toolbox::ScaleVt),
}

// --- Software filter structs ---

#[derive(Debug, Clone)]
pub struct HwUploadFilter {
    pub target_surface: FrameSurface,
    pub source_format: PixelFormat,
}

impl VideoFilterOp for HwUploadFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.surface = self.target_surface;
        state.pixel_format = match &state.pixel_format {
            PixelFormat::Yuv420p10le => PixelFormat::P010le,
            PixelFormat::Yuv420p => PixelFormat::Nv12,
            PixelFormat::Bgra if state.surface == FrameSurface::Cuda => PixelFormat::Yuva420p,
            other => *other,
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        None
    }

    fn as_arg(&self) -> Option<String> {
        let target_format = match (
            &self.target_surface,
            self.source_format.bit_depth(),
            self.source_format.has_alpha(),
        ) {
            (_, 10, _) => PixelFormat::P010le,
            (FrameSurface::Cuda, 8, true) => PixelFormat::Yuva420p,
            (FrameSurface::Vaapi, 8, true) => PixelFormat::Bgra,
            (FrameSurface::Qsv, 8, true) => PixelFormat::Bgra,
            _ => PixelFormat::Nv12,
        };

        let format_filter = if self.source_format == target_format {
            String::new()
        } else {
            format!("format={},", target_format.as_arg())
        };

        match &self.target_surface {
            FrameSurface::Cuda => Some(format!("{format_filter}hwupload_cuda")),
            FrameSurface::Rkmpp => Some(format!("{format_filter}hwupload")),

            #[cfg(target_os = "windows")]
            FrameSurface::Qsv => Some(format!("{format_filter}hwupload=extra_hw_frames=64")),

            #[cfg(not(target_os = "windows"))]
            FrameSurface::Qsv => Some(format!("{format_filter}hwupload")),

            FrameSurface::Vaapi => Some(format!("{format_filter}hwupload")),
            FrameSurface::Vulkan => Some(format!("{format_filter}hwupload")),
            FrameSurface::VideoToolbox => Some(format!("{format_filter}hwupload")),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HwDownloadFilter {
    pub target_pixel_format: PixelFormat,
}

impl VideoFilterOp for HwDownloadFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.surface = FrameSurface::System;
        state.pixel_format = self.target_pixel_format;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        None
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!(
            "hwdownload,format={}",
            self.target_pixel_format.as_arg()
        ))
    }
}

#[derive(Debug, Clone)]
pub struct ScaleFilter {
    pub size: Option<FrameSize>,
    pub scaling_mode: ScalingMode,
    pub input_is_anamorphic: bool,
    pub force_original_aspect_ratio: Option<ForceOriginalAspectRatio>,
}

impl VideoFilterOp for ScaleFilter {
    fn evaluate(&self, state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        let target = self.size?;
        if state.size == target && !state.is_anamorphic {
            return None;
        }

        // no need to scale "cropped" content that is already large enough
        if self.scaling_mode == ScalingMode::Crop
            && state.size.height >= target.height
            && state.size.width >= target.width
        {
            return None;
        }

        let (size, force) = match self.scaling_mode {
            ScalingMode::ScaleAndPad => {
                let actual = target.square_pixel_size_contain(state);
                let force = (actual != target).then_some(ForceOriginalAspectRatio::Decrease);
                (actual, force)
            }
            ScalingMode::Stretch => (target, None),
            ScalingMode::Crop => (target.square_pixel_size_cover(state), None),
        };

        Some(
            ScaleFilter {
                size: Some(size),
                scaling_mode: self.scaling_mode,
                input_is_anamorphic: state.is_anamorphic,
                force_original_aspect_ratio: force,
            }
            .into(),
        )
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.is_anamorphic = false;
            state.sample_aspect_ratio = Some(String::from("1:1"));
            state.display_aspect_ratio = None;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        if let Some(size) = &self.size {
            let aspect_ratio = self
                .force_original_aspect_ratio
                .as_ref()
                .map_or(String::new(), |f| f.as_arg());

            if self.input_is_anamorphic {
                Some(format!(
                    "scale=iw*sar:ih,scale={}:{}:flags=fast_bilinear{},setsar=1",
                    size.width, size.height, aspect_ratio
                ))
            } else {
                Some(format!(
                    "scale={}:{}:flags=fast_bilinear{},setsar=1",
                    size.width, size.height, aspect_ratio
                ))
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct PadFilter {
    pub size: Option<FrameSize>,
    pub scaling_mode: ScalingMode,
}

impl VideoFilterOp for PadFilter {
    fn evaluate(&self, state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if self.scaling_mode != ScalingMode::ScaleAndPad {
            return None;
        }

        match &self.size {
            Some(target) if state.size != *target => Some(self.clone().into()),
            _ => None,
        }
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::System;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        self.size
            .as_ref()
            .map(|size| format!("pad={}:{}:-1:-1:color=black", size.width, size.height))
    }
}

#[derive(Debug, Clone)]
pub struct LoopFilter {
    pub is_still_image: bool,
}

impl VideoFilterOp for LoopFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if self.is_still_image {
            Some(self.clone().into())
        } else {
            None
        }
    }

    fn apply_to(&self, _state: &mut FrameState) {}

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        Some(String::from("loop=-1:1"))
    }
}

#[derive(Debug, Clone)]
pub struct FormatFilter {
    pub format: PixelFormat,
}

impl VideoFilterOp for FormatFilter {
    fn evaluate(&self, state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if state.pixel_format == self.format {
            None
        } else {
            Some(self.clone().into())
        }
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.format;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!("format={}", self.format.as_arg()))
    }
}

#[derive(Debug, Clone)]
pub struct ToneMapFilter {
    pub algorithm: Option<String>,
    pub output_format: PixelFormat,
}

impl VideoFilterOp for ToneMapFilter {
    fn evaluate(&self, state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if state.hdr_format != HdrFormat::None {
            Some(self.clone().into())
        } else {
            None
        }
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.output_format;
        state.hdr_format = HdrFormat::None;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!(
            "zscale=t=linear,zscale=p=bt709,tonemap={},zscale=p=bt709:t=bt709:m=bt709:r=tv,format={}",
            self.algorithm.as_deref().unwrap_or("linear"),
            self.output_format.as_arg()
        ))
    }
}

#[derive(Debug, Clone)]
pub struct DeinterlaceFilter {
    pub filter: SoftwareDeinterlaceFilter,
    pub options: SoftwareDeinterlaceOptions,
    pub input_is_interlaced: bool,
}

impl VideoFilterOp for DeinterlaceFilter {
    fn evaluate(&self, _state: &FrameState, ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if self.input_is_interlaced {
            let best = ffmpeg_info.find_best_fit(&[
                KnownVideoFilter::Yadif,
                KnownVideoFilter::Bwdif,
                KnownVideoFilter::W3fdif,
            ]);

            if let Some(known_filter) = best {
                let software_filter = match known_filter {
                    KnownVideoFilter::Yadif => {
                        SoftwareDeinterlaceFilter::Yadif(self.options.yadif.clone())
                    }
                    KnownVideoFilter::Bwdif => {
                        SoftwareDeinterlaceFilter::Bwdif(self.options.bwdif.clone())
                    }
                    KnownVideoFilter::W3fdif => {
                        SoftwareDeinterlaceFilter::W3fdif(self.options.w3fdif.clone())
                    }
                    _ => return None,
                };
                return Some(
                    DeinterlaceFilter {
                        filter: software_filter,
                        // unused after this point
                        options: SoftwareDeinterlaceOptions::default(),
                        input_is_interlaced: self.input_is_interlaced,
                    }
                    .into(),
                );
            }
        }

        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.is_interlaced = false;
        state.pixel_format = match state.pixel_format.bit_depth() {
            10 => PixelFormat::Yuv420p10le,
            _ => PixelFormat::Yuv420p,
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        match &self.filter {
            SoftwareDeinterlaceFilter::Yadif(options) => {
                let mode = options.mode.as_deref().unwrap_or("1");
                Some(format!("yadif={mode}"))
            }
            SoftwareDeinterlaceFilter::Bwdif(options) => {
                let mode = options.mode.as_deref().unwrap_or("1");
                Some(format!("bwdif={mode}"))
            }
            SoftwareDeinterlaceFilter::W3fdif(options) => {
                let mode = options.mode.as_deref().unwrap_or("1");
                Some(format!("w3fdif=mode={mode}"))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct HwMapFilter {
    pub from_surface: FrameSurface,
    pub to_surface: FrameSurface,
    pub reverse: bool,
}

impl VideoFilterOp for HwMapFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        None
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.surface = self.to_surface;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        None
    }

    fn as_arg(&self) -> Option<String> {
        let reverse_part = if self.reverse { ":reverse=1" } else { "" };
        self.to_surface
            .device_name()
            .map(|name| format!("hwmap=derive_device={name}{reverse_part}"))
    }
}

#[derive(Debug, Clone)]
pub struct SubtitlesFilter {
    pub path: String,
    pub seek: Duration,
    pub fonts_folder: Option<String>,
    pub force_style: Option<String>,
}

impl VideoFilterOp for SubtitlesFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if !self.path.is_empty() {
            Some(self.clone().into())
        } else {
            None
        }
    }

    fn apply_to(&self, _state: &mut FrameState) {
        // no change to state
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        let filter_args = match (self.fonts_folder.as_ref(), self.force_style.as_ref()) {
            (Some(fonts_folder), Some(force_style)) => format!(
                "{}:fontsdir={}:force_style={}",
                FfmpegInfo::escape_path(&self.path),
                FfmpegInfo::escape_path(fonts_folder),
                FfmpegInfo::escape_filter_value(force_style)
            ),
            (Some(fonts_folder), None) => format!(
                "{}:fontsdir={}",
                FfmpegInfo::escape_path(&self.path),
                FfmpegInfo::escape_path(fonts_folder),
            ),
            (None, Some(force_style)) => format!(
                "{}:force_style={}",
                FfmpegInfo::escape_path(&self.path),
                FfmpegInfo::escape_filter_value(force_style)
            ),
            (None, None) => FfmpegInfo::escape_path(&self.path),
        };

        if self.seek > Duration::ZERO {
            Some(format!(
                "setpts=PTS+{}/TB,subtitles={},setpts=PTS-STARTPTS",
                self.seek.as_secs_f64(),
                filter_args,
            ))
        } else {
            Some(format!("subtitles={}", filter_args))
        }
    }
}

#[derive(Debug, Clone)]
pub struct SubtitleImageScaleFilter {
    pub size: FrameSize,
}

impl VideoFilterOp for SubtitleImageScaleFilter {
    fn evaluate(&self, _state: &FrameState, _info: &FfmpegInfo) -> Option<VideoFilter> {
        Some(self.clone().into())
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.size = self.size;
        state.is_anamorphic = false;
        state.sample_aspect_ratio = Some(String::from("1:1"));
        state.display_aspect_ratio = None;
        state.surface = FrameSurface::System;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!(
            "scale={}:{}:flags=fast_bilinear:force_original_aspect_ratio=decrease,setsar=1",
            self.size.width, self.size.height,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct ColorChannelMixerFilter {
    pub alpha: f32,
}

impl VideoFilterOp for ColorChannelMixerFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if self.alpha == 1f32 {
            None
        } else {
            Some(self.clone().into())
        }
    }

    fn apply_to(&self, _state: &mut FrameState) {
        // no change to state
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!("colorchannelmixer=aa={}", self.alpha))
    }
}

/// Extends the video stream past its source's end by cloning the last frame,
/// so an under-running source still fills its item's scheduled duration; the
/// output -t clamp bounds the padding.
#[derive(Debug, Clone)]
pub struct TPadFilter;

impl VideoFilterOp for TPadFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        Some(self.clone().into())
    }

    fn apply_to(&self, _state: &mut FrameState) {
        // no change to state
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        Some(String::from("tpad=stop=-1:stop_mode=clone"))
    }
}

#[derive(Debug, Clone)]
pub struct FadeFilter {
    point: FadePoint,
    duration: Duration,
}

impl FadeFilter {
    pub fn for_graphics(
        timing: Option<&WatermarkTiming>,
        item_start: OffsetDateTime,
        playout_offset: Duration,
        duration: Duration,
    ) -> Vec<FadeFilter> {
        if let Some(WatermarkTiming::Periodic(timing)) = timing {
            let fade_duration = Duration::from_millis(timing.fade_ms.unwrap_or(1000));
            let points = FadePoint::periodic(timing, item_start, playout_offset, duration);
            points
                .iter()
                .map(|p| FadeFilter {
                    point: *p,
                    duration: fade_duration,
                })
                .collect()
        } else {
            Vec::new()
        }
    }
}

impl VideoFilterOp for FadeFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        Some(self.clone().into())
    }

    fn apply_to(&self, _state: &mut FrameState) {
        // no change to state
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        let in_out = match self.point.mode {
            FadeMode::In => "in",
            FadeMode::Out => "out",
        };

        Some(format!(
            "fade={in_out}:st={}:d={}:alpha=1:enable='between(t,{},{})'",
            self.point.time.as_secs_f64(),
            self.duration.as_secs_f64(),
            self.point.enable_start.as_secs_f64(),
            self.point.enable_finish.as_secs_f64(),
        ))
    }
}

#[derive(Debug, Clone, Copy)]
enum FadeMode {
    In,
    Out,
}

#[derive(Debug, Clone, Copy)]
struct FadePoint {
    mode: FadeMode,
    time: Duration,
    enable_start: Duration,
    enable_finish: Duration,
}

impl FadePoint {
    fn new(mode: FadeMode, time: Duration) -> Self {
        Self {
            mode,
            time,
            // periodic chaining assigns both bounds after collecting every point
            enable_start: Duration::ZERO,
            enable_finish: Duration::ZERO,
        }
    }

    pub fn periodic(
        timing: &PeriodicTiming,
        item_start: OffsetDateTime,
        playout_offset: Duration,
        duration: Duration,
    ) -> Vec<FadePoint> {
        let mut result = Vec::new();

        let interval_start = item_start + playout_offset;
        let interval_finish = interval_start + duration;

        let frequency = Duration::from_millis(timing.frequency_ms);
        let fade = Duration::from_millis(timing.fade_ms.unwrap_or(1000));
        let hold = Duration::from_millis(timing.hold_ms);

        if fade > hold || 2 * fade + hold > frequency {
            log::error!("graphics layer requires fade <= hold and 2 * fade + hold <= frequency");
            return result;
        }

        // find periodic base
        let mut current_time = match timing.clock {
            PeriodicClock::Content => {
                let phase_ms = timing.phase_offset_ms.unwrap_or(0);
                let offset_ms = playout_offset.as_millis() as u64;
                let cycle_ms = if offset_ms >= phase_ms {
                    phase_ms + ((offset_ms - phase_ms) / timing.frequency_ms) * timing.frequency_ms
                } else {
                    phase_ms
                };
                item_start + Duration::from_millis(cycle_ms)
            }
            PeriodicClock::Wall => {
                let phase = timing.phase_offset_ms.unwrap_or(0) as i64;
                let freq = timing.frequency_ms as i64;

                let interval_ms = (interval_start.unix_timestamp_nanos() / 1_000_000) as i64;

                let n = (interval_ms - phase).div_euclid(freq);
                let last_ms = n * freq + phase;

                OffsetDateTime::UNIX_EPOCH + Duration::from_millis(last_ms as u64)
            }
        };

        let stop_at = timing
            .disable_after_ms
            .map(|d| item_start + Duration::from_millis(d))
            .unwrap_or(interval_finish + frequency);

        let fade_ms = fade.as_millis() as i128;
        let hold_ms = hold.as_millis() as i128;

        // include the first cycle after this pipeline interval.
        // a future fade-in keeps alpha at zero when the entire interval falls between appearances.
        while current_time < stop_at && current_time < interval_finish + frequency {
            let delta_ms = (current_time - interval_start).whole_milliseconds();

            let fade_in_time_ms = delta_ms;
            let fade_out_time_ms = delta_ms + fade_ms + hold_ms;

            let fade_in_time = if fade_in_time_ms >= 0 {
                Some(Duration::from_millis(fade_in_time_ms as u64))
            } else {
                None
            };

            let fade_out_time = if fade_out_time_ms >= 0 {
                Some(Duration::from_millis(fade_out_time_ms as u64))
            } else {
                None
            };

            if let Some(t) = fade_in_time {
                result.push(FadePoint::new(FadeMode::In, t));
            }

            if let Some(t) = fade_out_time {
                result.push(FadePoint::new(FadeMode::Out, t));
            }

            current_time += frequency;
        }

        // with no remaining appearances (for example after disable_after), a
        // future fade-in establishes transparent alpha for the whole interval
        if result.is_empty() {
            result.push(FadePoint::new(FadeMode::In, duration + fade));
        }

        // overlap 'enable' windows on consecutive fades
        for i in 0..result.len() {
            result[i].enable_start = if i == 0 {
                Duration::ZERO
            } else {
                result[i - 1].time + fade
            };
        }

        for i in 0..result.len() {
            result[i].enable_finish = if i == result.len() - 1 {
                duration
            } else {
                result[i + 1].time.saturating_sub(fade)
            };
        }

        result.retain(|p| p.enable_start < p.enable_finish);

        result
    }
}

#[derive(Debug, Clone)]
pub struct CropFilter {
    pub size: Option<FrameSize>,
    pub scaling_mode: ScalingMode,
}

impl VideoFilterOp for CropFilter {
    fn evaluate(&self, state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if self.scaling_mode != ScalingMode::Crop {
            return None;
        }

        match &self.size {
            Some(target) if state.size != *target => Some(self.clone().into()),
            _ => None,
        }
    }

    fn apply_to(&self, state: &mut FrameState) {
        if let Some(size) = &self.size {
            state.size = *size;
            state.surface = FrameSurface::System;
        }
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        self.size
            .as_ref()
            .map(|size| format!("crop={}:{}", size.width, size.height))
    }
}

#[derive(Debug, Clone)]
pub struct Dv5WorkaroundFilter;

impl VideoFilterOp for Dv5WorkaroundFilter {
    fn evaluate(&self, state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if state.hdr_format == HdrFormat::Dv5 {
            Some(self.clone().into())
        } else {
            None
        }
    }

    fn apply_to(&self, _state: &mut FrameState) {}

    fn required_surface(&self) -> Option<FrameSurface> {
        None
    }

    fn as_arg(&self) -> Option<String> {
        Some(String::from(
            "setparams=color_trc=smpte2084:colorspace=bt2020nc:color_primaries=bt2020",
        ))
    }
}

#[cfg(test)]
mod tests {
    use time::{Date, Month, Time, UtcOffset};

    use super::*;

    #[test]
    fn hw_map_as_arg_produces_derive_device() {
        let filter: VideoFilter = HwMapFilter {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::OpenCL,
            reverse: false,
        }
        .into();
        assert_eq!(
            filter.as_arg(),
            Some(String::from("hwmap=derive_device=opencl"))
        );
    }

    #[test]
    fn hw_map_as_arg_reverse_direction() {
        let filter: VideoFilter = HwMapFilter {
            from_surface: FrameSurface::OpenCL,
            to_surface: FrameSurface::Vaapi,
            reverse: true,
        }
        .into();
        assert_eq!(
            filter.as_arg(),
            Some(String::from("hwmap=derive_device=vaapi:reverse=1"))
        );
    }

    #[test]
    fn hw_map_as_arg_returns_none_for_system() {
        let filter: VideoFilter = HwMapFilter {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::System,
            reverse: false,
        }
        .into();
        assert_eq!(filter.as_arg(), None);
    }

    #[test]
    fn hw_map_apply_to_updates_surface() {
        let mut state = FrameState {
            size: FrameSize {
                width: 1920,
                height: 1080,
            },
            is_anamorphic: false,
            is_interlaced: false,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            surface: FrameSurface::Vaapi,
            pixel_format: PixelFormat::P010le,
            hdr_format: HdrFormat::Pq,
        };

        let filter: VideoFilter = HwMapFilter {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::OpenCL,
            reverse: false,
        }
        .into();
        filter.apply_to(&mut state);

        assert_eq!(state.surface, FrameSurface::OpenCL);
        assert_eq!(state.pixel_format, PixelFormat::P010le);
        assert_eq!(state.hdr_format, HdrFormat::Pq);
    }

    #[test]
    fn hw_map_required_surface_is_none() {
        let filter: VideoFilter = HwMapFilter {
            from_surface: FrameSurface::Vaapi,
            to_surface: FrameSurface::OpenCL,
            reverse: false,
        }
        .into();
        assert_eq!(filter.required_surface(), None);
    }

    #[test]
    fn wall_clock_periodic_uses_the_current_playout_interval() {
        // every 5 min
        let timing = PeriodicTiming {
            clock: PeriodicClock::Wall,
            frequency_ms: 300_000,
            phase_offset_ms: Some(0),
            disable_after_ms: Some(3_000_000),
            fade_ms: Some(1_000),
            hold_ms: 8_000,
        };

        // starts at midnight
        let item_start = OffsetDateTime::new_in_offset(
            Date::from_calendar_date(2026, Month::May, 1).unwrap(),
            Time::from_hms(0, 0, 0).unwrap(),
            UtcOffset::from_hms(-5, 0, 0).unwrap(),
        );

        // join at 4:45 during the hidden portion. the next appearance begins
        // 15 seconds into this pipeline, not relative to the item's midnight start.
        let playout_offset = Duration::from_mins(4) + Duration::from_secs(45);
        let points =
            FadePoint::periodic(&timing, item_start, playout_offset, Duration::from_secs(44));

        assert!(matches!(points[0].mode, FadeMode::In));
        assert_eq!(points[0].time, Duration::from_secs(15));
    }

    #[test]
    fn wall_clock_periodic_stays_hidden_when_next_cycle_is_after_pipeline_end() {
        let timing = PeriodicTiming {
            clock: PeriodicClock::Wall,
            frequency_ms: 300_000,
            phase_offset_ms: Some(0),
            disable_after_ms: None,
            fade_ms: Some(1_000),
            hold_ms: 30_000,
        };
        let item_start = OffsetDateTime::UNIX_EPOCH;

        // join four minutes into the cycle and transcode only 44 seconds. the
        // future fade-in is retained so FFmpeg initializes alpha to transparent.
        let filters = FadeFilter::for_graphics(
            Some(&WatermarkTiming::Periodic(timing)),
            item_start,
            Duration::from_mins(4),
            Duration::from_secs(44),
        );
        let arg = filters[0].as_arg().unwrap();

        assert!(arg.contains("fade=in:st=60"), "unexpected fade: {arg}");
        assert!(
            arg.contains("between(t,0,90)"),
            "future fade must control alpha from pipeline start: {arg}"
        );
    }

    #[test]
    fn wall_clock_periodic_join_during_visible_phase_fades_out_on_schedule() {
        let timing = PeriodicTiming {
            clock: PeriodicClock::Wall,
            frequency_ms: 300_000,
            phase_offset_ms: Some(0),
            disable_after_ms: None,
            fade_ms: Some(1_000),
            hold_ms: 30_000,
        };
        let points = FadePoint::periodic(
            &timing,
            OffsetDateTime::UNIX_EPOCH,
            Duration::from_mins(5) + Duration::from_secs(5),
            Duration::from_secs(44),
        );

        assert!(matches!(points[0].mode, FadeMode::Out));
        assert_eq!(points[0].time, Duration::from_secs(26));
    }

    #[test]
    fn content_clock_fast_forwards_and_chains_multiple_fades() {
        let timing = PeriodicTiming {
            clock: PeriodicClock::Content,
            frequency_ms: 10_000,
            phase_offset_ms: Some(2_000),
            disable_after_ms: None,
            fade_ms: Some(1_000),
            hold_ms: 3_000,
        };

        // an hour into the item, the prior cycle began three seconds ago. its
        // fade-out is one second ahead, followed by the next cycle at seven seconds.
        let points = FadePoint::periodic(
            &timing,
            OffsetDateTime::UNIX_EPOCH,
            Duration::from_secs(3_605),
            Duration::from_secs(22),
        );

        assert!(matches!(points[0].mode, FadeMode::Out));
        assert_eq!(points[0].time, Duration::from_secs(1));
        assert!(matches!(points[1].mode, FadeMode::In));
        assert_eq!(points[1].time, Duration::from_secs(7));
        assert_eq!(points[1].enable_start, Duration::from_secs(2));
        assert_eq!(points[1].enable_finish, Duration::from_secs(10));
        assert!(matches!(points[2].mode, FadeMode::Out));
        assert_eq!(points[2].time, Duration::from_secs(11));
        assert_eq!(points[2].enable_start, Duration::from_secs(8));
        assert_eq!(points[2].enable_finish, Duration::from_secs(16));
    }
}
