use std::time::Duration;

use enum_dispatch::enum_dispatch;
use time::OffsetDateTime;

use crate::accel;
use crate::ffmpeg_info::{FfmpegInfo, KnownVideoFilter};
use crate::frame_size::FrameSize;
use crate::input::{PeriodicClock, PeriodicTiming, WatermarkTiming};
use crate::output_settings::ScalingMode;
use crate::pipeline::{FrameState, FrameSurface, PixelFormat};

#[derive(Clone)]
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

#[derive(Clone)]
pub enum SoftwareDeinterlaceFilter {
    Bwdif,
    Yadif,
    W3fdif,
}

#[enum_dispatch]
pub trait VideoFilterOp {
    fn evaluate(&self, state: &FrameState, ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter>;
    fn apply_to(&self, state: &mut FrameState);
    fn required_surface(&self) -> Option<FrameSurface>;
    fn as_arg(&self) -> Option<String>;
}

#[derive(Clone)]
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
    ColorChannelMixer(ColorChannelMixerFilter),
    Fade(FadeFilter),
    Crop(CropFilter),
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

#[derive(Clone)]
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
            _ => PixelFormat::Nv12,
        };

        let format_filter = if self.source_format == target_format {
            String::new()
        } else {
            format!("format={},", target_format.as_arg())
        };

        match &self.target_surface {
            FrameSurface::Cuda => Some(format!("{format_filter}hwupload_cuda")),
            FrameSurface::Qsv => Some(format!("{format_filter}hwupload=extra_hw_frames=64")),
            FrameSurface::Vaapi => Some(format!("{format_filter}hwupload")),
            FrameSurface::Vulkan => Some(format!("{format_filter}hwupload")),
            FrameSurface::VideoToolbox => Some(format!("{format_filter}hwupload")),
            _ => None,
        }
    }
}

#[derive(Clone)]
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

#[derive(Clone)]
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

#[derive(Clone)]
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

#[derive(Clone)]
pub struct LoopFilter {
    pub codec: String,
}

impl VideoFilterOp for LoopFilter {
    fn evaluate(&self, _state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if self.codec == "png" {
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

#[derive(Clone)]
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

#[derive(Clone)]
pub struct ToneMapFilter {
    pub algorithm: Option<String>,
    pub output_format: PixelFormat,
}

impl VideoFilterOp for ToneMapFilter {
    fn evaluate(&self, state: &FrameState, _ffmpeg_info: &FfmpegInfo) -> Option<VideoFilter> {
        if state.is_hdr {
            Some(self.clone().into())
        } else {
            None
        }
    }

    fn apply_to(&self, state: &mut FrameState) {
        state.pixel_format = self.output_format;
        state.is_hdr = false;
    }

    fn required_surface(&self) -> Option<FrameSurface> {
        Some(FrameSurface::System)
    }

    fn as_arg(&self) -> Option<String> {
        Some(format!(
            "zscale=transfer=linear,tonemap={},zscale=transfer=bt709,format={}",
            self.algorithm.as_deref().unwrap_or("linear"),
            self.output_format.as_arg()
        ))
    }
}

#[derive(Clone)]
pub struct DeinterlaceFilter {
    pub filter: SoftwareDeinterlaceFilter,
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
                    KnownVideoFilter::Yadif => SoftwareDeinterlaceFilter::Yadif,
                    KnownVideoFilter::Bwdif => SoftwareDeinterlaceFilter::Bwdif,
                    KnownVideoFilter::W3fdif => SoftwareDeinterlaceFilter::W3fdif,
                    _ => return None,
                };
                return Some(
                    DeinterlaceFilter {
                        filter: software_filter,
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
            SoftwareDeinterlaceFilter::Yadif => Some(String::from("yadif=1")),
            SoftwareDeinterlaceFilter::Bwdif => Some(String::from("bwdif=1")),
            SoftwareDeinterlaceFilter::W3fdif => Some(String::from("w3fdif=1")),
        }
    }
}

#[derive(Clone)]
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

#[derive(Clone)]
pub struct SubtitlesFilter {
    pub path: String,
    pub seek: Duration,
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
        let escaped_path = FfmpegInfo::escape_path(&self.path);

        if self.seek > Duration::ZERO {
            Some(format!(
                "setpts=PTS+{}/TB,subtitles={},setpts=PTS-STARTPTS",
                self.seek.as_secs_f64(),
                escaped_path,
            ))
        } else {
            Some(format!("subtitles={}", escaped_path))
        }
    }
}

#[derive(Clone)]
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

#[derive(Clone)]
pub struct FadeFilter {
    point: FadePoint,
    duration: Duration,
}

impl FadeFilter {
    pub fn for_watermark(
        timing: Option<&WatermarkTiming>,
        item_start: OffsetDateTime,
        in_point: Duration,
        out_point: Duration,
    ) -> Vec<FadeFilter> {
        if let Some(WatermarkTiming::Periodic(timing)) = timing {
            let duration = Duration::from_millis(timing.fade_ms.unwrap_or(1000));
            let points = FadePoint::periodic(timing, item_start, in_point, out_point);
            points
                .iter()
                .map(|p| FadeFilter {
                    point: *p,
                    duration,
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

#[derive(Clone, Copy)]
enum FadeMode {
    In,
    Out,
}

#[derive(Clone, Copy)]
struct FadePoint {
    mode: FadeMode,
    time: Duration,
    enable_start: Duration,
    enable_finish: Duration,
}

impl FadePoint {
    pub fn periodic(
        timing: &PeriodicTiming,
        item_start: OffsetDateTime,
        in_point: Duration,
        out_point: Duration,
    ) -> Vec<FadePoint> {
        let mut result = Vec::new();

        let duration = out_point - in_point;
        let item_finish = item_start + duration;

        let frequency = Duration::from_millis(timing.frequency_ms);
        let fade = Duration::from_millis(timing.fade_ms.unwrap_or(1000));
        let hold = Duration::from_millis(timing.hold_ms);

        if fade > hold || 2 * fade + hold > frequency {
            log::error!("watermark requires fade <= hold and 2 * fade + hold <= frequency");
            return result;
        }

        // find periodic base
        let mut current_time = match timing.clock {
            PeriodicClock::Content => {
                item_start + Duration::from_millis(timing.phase_offset_ms.unwrap_or(0))
            }
            PeriodicClock::Wall => {
                let phase = timing.phase_offset_ms.unwrap_or(0) as i64;
                let freq = timing.frequency_ms as i64;

                let item_ms = (item_start.unix_timestamp_nanos() / 1_000_000) as i64;

                let n = (item_ms - phase).div_euclid(freq);
                let last_ms = n * freq + phase;

                OffsetDateTime::UNIX_EPOCH + Duration::from_millis(last_ms as u64)
            }
        };

        let stop_at = timing
            .disable_after_ms
            .map(|d| item_start + Duration::from_millis(d))
            .unwrap_or(item_finish);

        let in_point_ms = in_point.as_millis() as i128;
        let fade_ms = fade.as_millis() as i128;
        let hold_ms = hold.as_millis() as i128;

        while current_time < stop_at {
            let delta_ms = (current_time - item_start).whole_milliseconds();

            let fade_in_time_ms = delta_ms - in_point_ms;
            let fade_out_time_ms = (delta_ms + fade_ms + hold_ms) - in_point_ms;

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

            if let Some(t) = fade_in_time
                && current_time >= item_start
            {
                result.push(FadePoint {
                    mode: FadeMode::In,
                    time: t,
                    enable_start: t,
                    enable_finish: (t + fade).min(duration),
                });
            }

            if let Some(t) = fade_out_time {
                result.push(FadePoint {
                    mode: FadeMode::Out,
                    time: t,
                    enable_start: t,
                    enable_finish: (t + fade).min(duration),
                });
            }

            current_time += frequency;
        }

        result.retain(|p| p.enable_start < p.enable_finish);

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

        result
    }
}

#[derive(Clone)]
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
            is_hdr: true,
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
        assert!(state.is_hdr);
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
    fn fade_point_periodic() {
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

        // join at 4:45
        let in_point = Duration::from_mins(4) + Duration::from_secs(45);

        let points = FadePoint::periodic(&timing, item_start, in_point, Duration::from_secs(734));

        assert_eq!(points.len(), 4);
        assert_eq!(points[0].time, Duration::from_secs(15));
        assert_eq!(points[2].time, Duration::from_secs(5 * 60 + 15));
    }
}
