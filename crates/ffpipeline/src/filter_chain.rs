use crate::ArgVec;
use crate::audio_filter::AudioFilter;
use crate::ffmpeg_info::FfmpegInfo;
use crate::hw_accel::{HardwareAccel, HwAccel};
use crate::overlay_filter::{OverlayFilter, OverlayKindOp, OverlaySource};
use crate::pipeline::{FrameState, FrameSurface, PixelFormat, SurfaceSet};
use crate::video_filter::{
    FormatFilter, HwDownloadFilter, HwUploadFilter, VideoFilter, VideoFilterOp,
};

#[derive(Clone)]
pub enum PipelineFilter {
    Audio(AudioFilter),
    Video(VideoFilter),
    Overlay(OverlayFilter),
}

#[derive(Clone)]
pub(crate) struct FilterChain {
    pub(crate) filters: Vec<PipelineFilter>,
    surfaces: SurfaceSet,
    audio_label: String,
    video_label: String,
    complex_filter: String,
}

impl FilterChain {
    pub(crate) fn new(filters: Vec<PipelineFilter>) -> FilterChain {
        FilterChain {
            filters,
            surfaces: SurfaceSet::new(),
            audio_label: String::new(),
            video_label: String::new(),
            complex_filter: String::new(),
        }
    }

    /// Disables/drops all audio filters from the filter chain.
    pub(crate) fn disable_audio(&mut self) {
        self.filters
            .retain(|f| !matches!(f, PipelineFilter::Audio(_)));
    }

    /// Disables/drops all video filters from the filter chain.
    pub(crate) fn disable_video(&mut self) {
        self.filters
            .retain(|f| !matches!(f, PipelineFilter::Video(_)));
    }

    /// Optimizes the filter chain by passing the frame state through each filter.
    /// Filters will be dropped when the input state already matches the desired output state.
    pub(crate) fn evaluate(&mut self, initial_state: &FrameState, ffmpeg_info: &FfmpegInfo) {
        let mut state = initial_state.to_owned();
        let mut active_filters = Vec::new();

        for filter in &self.filters {
            match filter {
                PipelineFilter::Audio(af) => {
                    if let Some(new_filter) = af.evaluate(&state) {
                        new_filter.apply_to(&mut state);
                        active_filters.push(PipelineFilter::Audio(new_filter));
                    }
                }
                PipelineFilter::Video(vf) => {
                    if let Some(new_filter) = vf.evaluate(&state, ffmpeg_info) {
                        new_filter.apply_to(&mut state);
                        active_filters.push(PipelineFilter::Video(new_filter));
                    }
                }
                PipelineFilter::Overlay(of) => {
                    let new_filter = of.clone();
                    new_filter.kind.apply_to(&mut state);
                    active_filters.push(PipelineFilter::Overlay(new_filter));
                }
            }
        }

        self.filters = active_filters;
    }

    /// Resolves the filter chain by walking each filter in order, tracking the
    /// current frame state (surface, pixel format, etc.) and inserting any
    /// surface transfers (hw download/upload/map) or pixel format conversions
    /// needed between filters or before the encoder.
    ///
    /// Each video filter is passed through the hardware accelerator's
    /// [`HwAccel::best_filter`] to select a hardware-optimized variant when
    /// available. After all filters are resolved, the final state is reconciled
    /// with the encoder's expected surface and pixel format, appending
    /// additional transfer or format filters as needed.
    pub(crate) fn resolve(
        &mut self,
        ffmpeg_info: &FfmpegInfo,
        accel: &Option<HardwareAccel>,
        initial_state: &FrameState,
        encoder_surface: &FrameSurface,
        encoder_pixel_format: &Option<PixelFormat>,
    ) {
        let mut resolved = Vec::new();
        let mut current_state = initial_state.clone();
        let mut surfaces = SurfaceSet::new();

        for filter in &self.filters {
            match filter {
                PipelineFilter::Video(video_filter) => {
                    let mut best = match accel {
                        Some(a) => a.best_filter(video_filter, ffmpeg_info, &current_state),
                        _ => video_filter.clone(),
                    };

                    if let Some(required) = best.required_surface()
                        && current_state.surface != required
                        && !Self::transfer_surface(
                            accel,
                            &mut resolved,
                            &mut current_state,
                            required,
                            encoder_pixel_format,
                            &mut surfaces,
                        )
                    {
                        best = video_filter.clone();
                    }

                    best.apply_to(&mut current_state);
                    surfaces.insert(current_state.surface);
                    resolved.push(PipelineFilter::Video(best));
                }
                PipelineFilter::Audio(_) => {
                    // not sure if we should actually apply audio filters to the state
                    // since they are separate in the real filter graph
                    //audio_filter.apply_to(&mut current_state);
                    resolved.push(filter.clone());
                }
                PipelineFilter::Overlay(overlay) => {
                    let mut best = match accel {
                        Some(a) => a.best_overlay(overlay, ffmpeg_info, &current_state),
                        _ => overlay.clone(),
                    };

                    // ensure main input matches overlay required surface
                    let main_req = best.kind.main_input_state(&current_state);
                    if current_state.surface != main_req.surface {
                        Self::transfer_surface(
                            accel,
                            &mut resolved,
                            &mut current_state,
                            main_req.surface,
                            encoder_pixel_format,
                            &mut surfaces,
                        );
                    }

                    // ensure main input matches overlay required pixel format
                    if current_state.pixel_format != main_req.pixel_format {
                        Self::convert_pixel_format(
                            &mut resolved,
                            &mut current_state,
                            &main_req.pixel_format,
                            accel,
                        );
                    }

                    // ensure secondary input matches overlay required surface, pixel format
                    let sec_req = best.kind.secondary_input_state(&current_state);
                    let mut sec = FilterChain::new(
                        best.secondary
                            .iter()
                            .cloned()
                            .map(PipelineFilter::Video)
                            .collect(),
                    );
                    sec.evaluate(&best.secondary_initial_state, ffmpeg_info);

                    // ignore hw accel if secondary needs to stay in software anyway
                    let sec_accel = if sec_req.surface == FrameSurface::System {
                        &None
                    } else {
                        accel
                    };

                    sec.resolve(
                        ffmpeg_info,
                        sec_accel,
                        &best.secondary_initial_state,
                        &sec_req.surface,
                        &Some(sec_req.pixel_format),
                    );
                    best.secondary = sec
                        .filters
                        .into_iter()
                        .filter_map(|f| match f {
                            PipelineFilter::Video(v) => Some(v),
                            _ => None,
                        })
                        .collect();

                    // track all secondary surfaces
                    surfaces.extend(sec.surfaces.iter().copied());

                    best.kind.apply_to(&mut current_state);
                    surfaces.insert(current_state.surface);
                    resolved.push(PipelineFilter::Overlay(best));
                }
            }
        }

        if current_state.surface != *encoder_surface {
            log::debug!(
                "current surface {:?} doesn't match encoder {:?}",
                current_state.surface,
                *encoder_surface
            );

            if !Self::transfer_surface(
                accel,
                &mut resolved,
                &mut current_state,
                *encoder_surface,
                encoder_pixel_format,
                &mut surfaces,
            ) {
                log::error!("failed to transfer surface to encoder");
            }
        }

        if let Some(pixel_format) = encoder_pixel_format
            && current_state.pixel_format != *pixel_format
        {
            Self::convert_pixel_format(&mut resolved, &mut current_state, pixel_format, accel);
        }

        self.filters = resolved;
        self.surfaces = surfaces;
    }

    pub(crate) fn surfaces(&self) -> &SurfaceSet {
        &self.surfaces
    }

    fn transfer_surface(
        accel: &Option<HardwareAccel>,
        resolved: &mut Vec<PipelineFilter>,
        current_state: &mut FrameState,
        required: FrameSurface,
        encoder_pixel_format: &Option<PixelFormat>,
        surfaces: &mut SurfaceSet,
    ) -> bool {
        log::trace!(
            "Determining surface transfer. State: {}, accel: {:?}, required surface: {}",
            current_state,
            accel,
            required
        );
        // Check if we're moving down to system (software) frames
        if required == FrameSurface::System {
            let target_pixel_format = match current_state.pixel_format.bit_depth() {
                10 => PixelFormat::P010le,
                _ => PixelFormat::Nv12,
            };

            let download: VideoFilter = HwDownloadFilter {
                target_pixel_format,
            }
            .into();
            download.apply_to(current_state);
            surfaces.insert(current_state.surface);
            resolved.push(PipelineFilter::Video(download));
            return true;
        }

        // If we're moving into hardware from software
        // first check if the current pixel formats are compatiable, otherwise
        // we will need explicit converesion
        if current_state.surface == FrameSurface::System {
            let is_format_supported = match accel {
                Some(a) => a.supports_pixel_format(&current_state.pixel_format),
                None => true,
            };

            if is_format_supported {
                let upload: VideoFilter = HwUploadFilter {
                    target_surface: required,
                    source_format: current_state.pixel_format,
                }
                .into();
                upload.apply_to(current_state);
                surfaces.insert(current_state.surface);
                resolved.push(PipelineFilter::Video(upload));
                return true;
            } else if current_state.pixel_format.bit_depth() == 10
                && encoder_pixel_format
                    .as_ref()
                    .is_some_and(|pf| pf.bit_depth() == 8)
            {
                let format: VideoFilter = FormatFilter {
                    format: PixelFormat::Nv12,
                }
                .into();
                format.apply_to(current_state);
                resolved.push(PipelineFilter::Video(format));

                let upload: VideoFilter = HwUploadFilter {
                    target_surface: required,
                    source_format: current_state.pixel_format,
                }
                .into();
                upload.apply_to(current_state);
                surfaces.insert(current_state.surface);
                resolved.push(PipelineFilter::Video(upload));
                return true;
            } else {
                return false;
            }
        }

        // Lastly, if we're doing a hw -> hw transition, see if we can do so
        // using the accel's hwmap impl.
        if let Some(map) = accel
            .as_ref()
            .and_then(|a| a.hw_map_filter(&current_state.surface, &required))
        {
            map.apply_to(current_state);
            surfaces.insert(current_state.surface);
            resolved.push(PipelineFilter::Video(map));
            return true;
        }

        false
    }

    fn convert_pixel_format(
        resolved: &mut Vec<PipelineFilter>,
        current_state: &mut FrameState,
        pixel_format: &PixelFormat,
        accel: &Option<HardwareAccel>,
    ) {
        log::debug!(
            "current pixel format {:?} doesn't match required {:?}",
            current_state.pixel_format,
            *pixel_format
        );

        match (&current_state.surface, accel) {
            (FrameSurface::System, _) => {
                let format: VideoFilter = FormatFilter {
                    format: pixel_format.to_owned(),
                }
                .into();
                format.apply_to(current_state);
                resolved.push(PipelineFilter::Video(format))
            }
            (_, Some(a)) => {
                if let Some(f) = a.format_filter(pixel_format) {
                    f.apply_to(current_state);
                    resolved.push(PipelineFilter::Video(f));
                }
            }
            _ => {}
        }
    }

    pub(crate) fn optimize(&mut self) {
        // swap software scale before software tone map to reduce
        // the amount of data that needs to be tone mapped
        if let Some(tonemap_index) = self
            .filters
            .iter()
            .position(|f| matches!(f, PipelineFilter::Video(VideoFilter::ToneMap(_))))
            && let Some(PipelineFilter::Video(VideoFilter::Scale(_))) =
                self.filters.get(tonemap_index + 1)
        {
            log::debug!("swapping software scale filter before software tonemap filter");
            self.filters.swap(tonemap_index, tonemap_index + 1);
        }
    }

    pub(crate) fn build(
        &mut self,
        audio_label: &str,
        video_label: &str,
        subtitle_label: Option<&String>,
        watermark_label: Option<&String>,
    ) {
        self.audio_label = audio_label.to_owned();
        self.video_label = video_label.to_owned();

        let mut filter_chains: Vec<String> = Vec::new();

        // build filter chain
        let audio_filter_count = self
            .filters
            .iter()
            .filter(|f| matches!(f, PipelineFilter::Audio(_)))
            .count();

        if audio_filter_count > 0 {
            let mut filter_chain = String::new();

            filter_chain.push_str(&format!("[{}]", self.audio_label));

            let mut audio_chain: Vec<String> = Vec::new();

            for filter in self.filters.iter() {
                if let PipelineFilter::Audio(audio_filter) = filter
                    && let Some(arg) = audio_filter.as_arg()
                {
                    audio_chain.push(arg)
                }
            }

            filter_chain.push_str(&audio_chain.join(","));

            self.audio_label = String::from("[a]");
            filter_chain.push_str(&self.audio_label);

            filter_chains.push(filter_chain);
        }

        let video_filter_count = self
            .filters
            .iter()
            .filter(|f| matches!(f, PipelineFilter::Video(_) | PipelineFilter::Overlay(_)))
            .count();

        if video_filter_count > 0 {
            let mut current_in = self.video_label.clone();
            let mut pending: Vec<String> = Vec::new();
            let mut overlay_num: usize = 0;

            let flush =
                |chains: &mut Vec<String>, pending: &mut Vec<String>, from: &str, to: &str| {
                    chains.push(format!("[{}]{}[{}]", from, pending.join(","), to));
                    pending.clear();
                };

            for filter in &self.filters {
                match filter {
                    PipelineFilter::Video(video_filter) => {
                        if let Some(arg) = video_filter.as_arg() {
                            pending.push(arg);
                        }
                    }
                    PipelineFilter::Overlay(overlay) => {
                        let sec_label = match overlay.secondary_source {
                            OverlaySource::Subtitle => subtitle_label,
                            OverlaySource::Watermark => watermark_label,
                        };

                        let Some(sec_in) = sec_label else {
                            continue;
                        };

                        let main_label = format!("v_m{}", overlay_num);
                        if !pending.is_empty() {
                            flush(&mut filter_chains, &mut pending, &current_in, &main_label);
                            current_in = main_label;
                        }

                        let sec_args: Vec<String> = overlay
                            .secondary
                            .iter()
                            .filter_map(|f| f.as_arg())
                            .collect();
                        let sec_ref = if sec_args.is_empty() {
                            sec_in.to_owned()
                        } else {
                            let sec_label = format!("v_s{}", overlay_num);
                            filter_chains.push(format!(
                                "[{}]{}[{}]",
                                sec_in,
                                sec_args.join(","),
                                sec_label
                            ));
                            sec_label
                        };

                        let out_label = format!("v_o{}", overlay_num);
                        if let Some(arg) = overlay.kind.as_arg(overlay.location.clone()) {
                            filter_chains.push(format!(
                                "[{}][{}]{}[{}]",
                                current_in, sec_ref, arg, out_label
                            ));
                        }
                        current_in = out_label;
                        overlay_num += 1;
                    }
                    _ => {}
                }
            }

            if !pending.is_empty() {
                flush(&mut filter_chains, &mut pending, &current_in, "v");
                self.video_label = String::from("[v]");
            } else {
                self.video_label = format!("[{}]", current_in);
            }
        }

        self.complex_filter = filter_chains.join(";");
    }

    pub(crate) fn audio_label(&self) -> &str {
        &self.audio_label
    }

    pub(crate) fn video_label(&self) -> &str {
        &self.video_label
    }

    pub(crate) fn as_arg(&self) -> ArgVec {
        if self.complex_filter.is_empty() {
            Vec::new()
        } else {
            args!["-filter_complex", self.complex_filter.to_owned(),]
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::accel::opencl::{PadOpencl, TonemapOpencl};
    use crate::accel::vaapi::{PadVaapi, TonemapVaapi, Vaapi, VaapiDriver};
    use crate::capabilities::opencl::OpenCLCapabilities;
    use crate::capabilities::vaapi::VaapiCapabilities;
    use crate::ffmpeg_info::KnownVideoFilter;
    use crate::frame_size::FrameSize;
    use crate::hw_accel::HardwareAccel;
    use crate::pipeline::HwPixelFormat;
    use crate::video_filter::{HwMapFilter, PadFilter, ToneMapFilter};

    fn vaapi_accel() -> HardwareAccel {
        HardwareAccel::Vaapi(Vaapi {
            device: String::from("/dev/dri/renderD128"),
            driver: VaapiDriver::Ihd,
            capabilities: VaapiCapabilities {
                vendor: String::from("test"),
                supported: HashSet::new(),
                vpp_pixel_formats: HashSet::new(),
                can_hdr_to_sdr_tonemap: HashSet::new(),
                can_hdr_to_hdr_tonemap: HashSet::new(),
                can_overlay: false,
            },
            opencl_capabilities: OpenCLCapabilities::default(),
        })
    }

    fn vaapi_accel_with_tonemap(
        driver: VaapiDriver,
        hdr_to_sdr: bool,
        hdr_to_hdr: bool,
        opencl: bool,
    ) -> Vaapi {
        let mut can_hdr_to_sdr_tonemap = HashSet::new();
        let mut can_hdr_to_hdr_tonemap = HashSet::new();
        if hdr_to_sdr {
            can_hdr_to_sdr_tonemap.insert(libva_sys::VA_FOURCC_P010);
        }
        if hdr_to_hdr {
            can_hdr_to_hdr_tonemap.insert(libva_sys::VA_FOURCC_P010);
        }
        Vaapi {
            device: String::from("/dev/dri/renderD128"),
            driver,
            capabilities: VaapiCapabilities {
                vendor: String::from("test"),
                supported: HashSet::new(),
                vpp_pixel_formats: HashSet::new(),
                can_hdr_to_sdr_tonemap,
                can_hdr_to_hdr_tonemap,
                can_overlay: false,
            },
            opencl_capabilities: OpenCLCapabilities {
                platform_count: if opencl { 1 } else { 0 },
                gpu_device_count: if opencl { 1 } else { 0 },
            },
        }
    }

    fn ffmpeg_info_with_filters(filters: &[KnownVideoFilter]) -> FfmpegInfo {
        let mut video_filters = HashSet::new();
        for f in filters {
            video_filters.insert(f.to_string());
        }
        FfmpegInfo {
            hwaccels: HashSet::new(),
            video_filters,
            preferred_filters: HashMap::new(),
        }
    }

    fn hdr_vaapi_state() -> FrameState {
        FrameState {
            size: FrameSize {
                width: 3840,
                height: 2160,
            },
            is_anamorphic: false,
            is_interlaced: false,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            surface: FrameSurface::Vaapi,
            pixel_format: PixelFormat::P010le,
            is_hdr: true,
        }
    }

    #[test]
    fn resolve_inserts_hwmap_for_opencl_tonemap_on_vaapi() {
        let accel = vaapi_accel();
        let initial_state = hdr_vaapi_state();
        let ffmpeg_info = FfmpegInfo::default();

        let tonemap: VideoFilter = TonemapOpencl {
            algorithm: Some(String::from("hable")),
            output_format: HwPixelFormat::Nv12,
        }
        .into();

        let mut chain = FilterChain::new(vec![PipelineFilter::Video(tonemap)]);

        chain.resolve(
            &ffmpeg_info,
            &Some(accel),
            &initial_state,
            &FrameSurface::Vaapi,
            &Some(PixelFormat::Nv12),
        );

        let video_filters: Vec<&VideoFilter> = chain
            .filters
            .iter()
            .filter_map(|f| match f {
                PipelineFilter::Video(vf) => Some(vf),
                _ => None,
            })
            .collect();

        assert!(
            video_filters.len() >= 3,
            "expected at least 3 video filters (hwmap + tonemap + hwmap), got {}",
            video_filters.len()
        );

        assert!(
            matches!(
                video_filters[0],
                VideoFilter::HwMap(HwMapFilter {
                    from_surface: FrameSurface::Vaapi,
                    to_surface: FrameSurface::OpenCL,
                    reverse: false
                })
            ),
            "first filter should be HwMap from Vaapi to OpenCL"
        );

        assert!(
            matches!(video_filters[1], VideoFilter::TonemapOpencl(_)),
            "second filter should be the tonemap opencl filter"
        );

        assert!(
            matches!(
                video_filters[2],
                VideoFilter::HwMap(HwMapFilter {
                    from_surface: FrameSurface::OpenCL,
                    to_surface: FrameSurface::Vaapi,
                    reverse: true,
                })
            ),
            "third filter should be HwMap from OpenCL back to Vaapi"
        );
    }

    #[test]
    fn resolve_and_build_produces_correct_filter_complex_for_opencl_tonemap() {
        let accel = vaapi_accel();
        let initial_state = hdr_vaapi_state();
        let ffmpeg_info = FfmpegInfo::default();

        let tonemap: VideoFilter = TonemapOpencl {
            algorithm: Some(String::from("hable")),
            output_format: HwPixelFormat::Nv12,
        }
        .into();

        let mut chain = FilterChain::new(vec![PipelineFilter::Video(tonemap)]);

        chain.resolve(
            &ffmpeg_info,
            &Some(accel),
            &initial_state,
            &FrameSurface::Vaapi,
            &Some(PixelFormat::Nv12),
        );
        chain.build("0:a", "0:v", None, None);

        let args = chain.as_arg();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "-filter_complex");

        let filter_complex = &args[1];
        assert!(
            filter_complex.contains("hwmap=derive_device=opencl"),
            "filter_complex should contain hwmap to opencl: {filter_complex}"
        );
        assert!(
            filter_complex.contains("tonemap_opencl="),
            "filter_complex should contain tonemap_opencl: {filter_complex}"
        );
        assert!(
            filter_complex.contains("hwmap=derive_device=vaapi"),
            "filter_complex should contain hwmap back to vaapi: {filter_complex}"
        );

        let expected_order = "hwmap=derive_device=opencl,tonemap_opencl=";
        assert!(
            filter_complex.contains(expected_order),
            "hwmap to opencl should appear immediately before tonemap_opencl: {filter_complex}"
        );
    }

    #[test]
    fn resolve_does_not_insert_hwmap_when_surfaces_match() {
        let accel = vaapi_accel();
        let initial_state = hdr_vaapi_state();
        let ffmpeg_info = FfmpegInfo::default();

        let format_filter: VideoFilter = FormatFilter {
            format: PixelFormat::Nv12,
        }
        .into();

        let mut chain = FilterChain::new(vec![PipelineFilter::Video(format_filter)]);

        chain.resolve(
            &ffmpeg_info,
            &Some(accel),
            &initial_state,
            &FrameSurface::Vaapi,
            &Some(PixelFormat::Nv12),
        );

        let has_hwmap = chain
            .filters
            .iter()
            .any(|f| matches!(f, PipelineFilter::Video(VideoFilter::HwMap(_))));

        assert!(
            !has_hwmap,
            "no HwMap should be inserted when no hw-to-hw transition is needed"
        );
    }

    // -- tonemap_vaapi best_filter tests --

    #[test]
    fn best_filter_selects_tonemap_vaapi_for_hdr_to_sdr_when_capable() {
        let vaapi = vaapi_accel_with_tonemap(VaapiDriver::RadeonSI, true, false, false);
        let ffmpeg_info = ffmpeg_info_with_filters(&[KnownVideoFilter::TonemapVaapi]);
        let state = hdr_vaapi_state();

        let input: VideoFilter = ToneMapFilter {
            algorithm: Some(String::from("hable")),
            output_format: PixelFormat::Nv12,
        }
        .into();

        let result = vaapi.best_filter(&input, &ffmpeg_info, &state);
        assert!(
            matches!(
                result,
                VideoFilter::TonemapVaapi(TonemapVaapi {
                    output_format: HwPixelFormat::Nv12
                })
            ),
            "expected TonemapVaapi with Nv12 output, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn best_filter_selects_tonemap_vaapi_for_hdr_to_hdr_when_capable() {
        let vaapi = vaapi_accel_with_tonemap(VaapiDriver::RadeonSI, false, true, false);
        let ffmpeg_info = ffmpeg_info_with_filters(&[KnownVideoFilter::TonemapVaapi]);
        let state = hdr_vaapi_state();

        let input: VideoFilter = ToneMapFilter {
            algorithm: Some(String::from("hable")),
            output_format: PixelFormat::P010le,
        }
        .into();

        let result = vaapi.best_filter(&input, &ffmpeg_info, &state);
        assert!(
            matches!(
                result,
                VideoFilter::TonemapVaapi(TonemapVaapi {
                    output_format: HwPixelFormat::P010le
                })
            ),
            "expected TonemapVaapi with P010le output, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn best_filter_falls_back_to_software_tonemap_without_vaapi_capability() {
        let vaapi = vaapi_accel_with_tonemap(VaapiDriver::RadeonSI, false, false, false);
        let ffmpeg_info = ffmpeg_info_with_filters(&[KnownVideoFilter::TonemapVaapi]);
        let state = hdr_vaapi_state();

        let input: VideoFilter = ToneMapFilter {
            algorithm: Some(String::from("hable")),
            output_format: PixelFormat::Nv12,
        }
        .into();

        let result = vaapi.best_filter(&input, &ffmpeg_info, &state);
        assert!(
            matches!(result, VideoFilter::ToneMap(_)),
            "expected software ToneMap fallback, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn best_filter_prefers_opencl_over_vaapi_when_available() {
        let vaapi = vaapi_accel_with_tonemap(VaapiDriver::Ihd, true, false, true);
        let ffmpeg_info = ffmpeg_info_with_filters(&[
            KnownVideoFilter::TonemapOpencl,
            KnownVideoFilter::TonemapVaapi,
        ]);
        let state = hdr_vaapi_state();

        let input: VideoFilter = ToneMapFilter {
            algorithm: Some(String::from("hable")),
            output_format: PixelFormat::Nv12,
        }
        .into();

        let result = vaapi.best_filter(&input, &ffmpeg_info, &state);
        assert!(
            matches!(result, VideoFilter::TonemapOpencl(_)),
            "expected TonemapOpencl on iHD when both are available, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn best_filter_uses_vaapi_tonemap_when_opencl_unavailable() {
        let vaapi = vaapi_accel_with_tonemap(VaapiDriver::Ihd, true, false, false);
        let ffmpeg_info = ffmpeg_info_with_filters(&[KnownVideoFilter::TonemapVaapi]);
        let state = hdr_vaapi_state();

        let input: VideoFilter = ToneMapFilter {
            algorithm: Some(String::from("hable")),
            output_format: PixelFormat::Nv12,
        }
        .into();

        let result = vaapi.best_filter(&input, &ffmpeg_info, &state);
        assert!(
            matches!(result, VideoFilter::TonemapVaapi(_)),
            "expected TonemapVaapi on iHD when opencl is unavailable, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn best_filter_falls_back_to_software_when_no_hw_tonemap_filter() {
        let vaapi = vaapi_accel_with_tonemap(VaapiDriver::RadeonSI, true, true, false);
        let ffmpeg_info = ffmpeg_info_with_filters(&[]);
        let state = hdr_vaapi_state();

        let input: VideoFilter = ToneMapFilter {
            algorithm: Some(String::from("hable")),
            output_format: PixelFormat::Nv12,
        }
        .into();

        let result = vaapi.best_filter(&input, &ffmpeg_info, &state);
        assert!(
            matches!(result, VideoFilter::ToneMap(_)),
            "expected software ToneMap when ffmpeg has no hw tonemap filters, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn best_filter_falls_back_for_non_p010le_input() {
        let vaapi = vaapi_accel_with_tonemap(VaapiDriver::RadeonSI, true, true, false);
        let ffmpeg_info = ffmpeg_info_with_filters(&[KnownVideoFilter::TonemapVaapi]);

        let mut state = hdr_vaapi_state();
        state.pixel_format = PixelFormat::Nv12;

        let input: VideoFilter = ToneMapFilter {
            algorithm: Some(String::from("hable")),
            output_format: PixelFormat::Nv12,
        }
        .into();

        let result = vaapi.best_filter(&input, &ffmpeg_info, &state);
        assert!(
            matches!(result, VideoFilter::ToneMap(_)),
            "expected software ToneMap for non-P010le input, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn resolve_tonemap_vaapi_does_not_insert_hwmap() {
        let vaapi = vaapi_accel_with_tonemap(VaapiDriver::RadeonSI, true, false, false);
        let accel = HardwareAccel::Vaapi(vaapi);
        let ffmpeg_info = ffmpeg_info_with_filters(&[KnownVideoFilter::TonemapVaapi]);
        let initial_state = hdr_vaapi_state();

        let tonemap: VideoFilter = ToneMapFilter {
            algorithm: Some(String::from("hable")),
            output_format: PixelFormat::Nv12,
        }
        .into();

        let mut chain = FilterChain::new(vec![PipelineFilter::Video(tonemap)]);
        chain.resolve(
            &ffmpeg_info,
            &Some(accel),
            &initial_state,
            &FrameSurface::Vaapi,
            &Some(PixelFormat::Nv12),
        );

        let video_filters: Vec<&VideoFilter> = chain
            .filters
            .iter()
            .filter_map(|f| match f {
                PipelineFilter::Video(vf) => Some(vf),
                _ => None,
            })
            .collect();

        let has_hwmap = video_filters
            .iter()
            .any(|f| matches!(f, VideoFilter::HwMap(_)));
        assert!(
            !has_hwmap,
            "tonemap_vaapi stays on VAAPI surface, no HwMap needed"
        );

        assert!(
            matches!(video_filters[0], VideoFilter::TonemapVaapi(_)),
            "first video filter should be TonemapVaapi, got {:?}",
            video_filters[0].as_arg()
        );
    }

    #[test]
    fn resolve_and_build_produces_correct_filter_complex_for_vaapi_tonemap() {
        let vaapi = vaapi_accel_with_tonemap(VaapiDriver::RadeonSI, true, false, false);
        let accel = HardwareAccel::Vaapi(vaapi);
        let ffmpeg_info = ffmpeg_info_with_filters(&[KnownVideoFilter::TonemapVaapi]);
        let initial_state = hdr_vaapi_state();

        let tonemap: VideoFilter = ToneMapFilter {
            algorithm: Some(String::from("hable")),
            output_format: PixelFormat::Nv12,
        }
        .into();

        let mut chain = FilterChain::new(vec![PipelineFilter::Video(tonemap)]);
        chain.resolve(
            &ffmpeg_info,
            &Some(accel),
            &initial_state,
            &FrameSurface::Vaapi,
            &Some(PixelFormat::Nv12),
        );
        chain.build("0:a", "0:v", None, None);

        let args = chain.as_arg();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "-filter_complex");

        let filter_complex = &args[1];
        assert!(
            filter_complex.contains("tonemap_vaapi=format=nv12:t=bt709:m=bt709:p=bt709"),
            "filter_complex should contain tonemap_vaapi: {filter_complex}"
        );
        assert!(
            !filter_complex.contains("hwmap"),
            "filter_complex should not contain hwmap for vaapi tonemap: {filter_complex}"
        );
    }

    // -- pad best_filter tests --

    fn vaapi_accel_with_opencl(opencl: bool) -> Vaapi {
        Vaapi {
            device: String::from("/dev/dri/renderD128"),
            driver: VaapiDriver::Ihd,
            capabilities: VaapiCapabilities {
                vendor: String::from("test"),
                supported: HashSet::new(),
                vpp_pixel_formats: HashSet::new(),
                can_hdr_to_sdr_tonemap: HashSet::new(),
                can_hdr_to_hdr_tonemap: HashSet::new(),
                can_overlay: false,
            },
            opencl_capabilities: OpenCLCapabilities {
                platform_count: if opencl { 1 } else { 0 },
                gpu_device_count: if opencl { 1 } else { 0 },
            },
        }
    }

    fn sdr_vaapi_state() -> FrameState {
        FrameState {
            size: FrameSize {
                width: 1280,
                height: 720,
            },
            is_anamorphic: false,
            is_interlaced: false,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            surface: FrameSurface::Vaapi,
            pixel_format: PixelFormat::Nv12,
            is_hdr: false,
        }
    }

    #[test]
    fn best_filter_selects_pad_vaapi_when_available() {
        let vaapi = vaapi_accel_with_opencl(true);
        let ffmpeg_info =
            ffmpeg_info_with_filters(&[KnownVideoFilter::PadVaapi, KnownVideoFilter::PadOpencl]);
        let state = sdr_vaapi_state();

        let input: VideoFilter = PadFilter {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
        }
        .into();

        let result = vaapi.best_filter(&input, &ffmpeg_info, &state);
        assert!(
            matches!(result, VideoFilter::PadVaapi(PadVaapi { .. })),
            "expected PadVaapi when both pad_vaapi and pad_opencl available, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn best_filter_selects_pad_opencl_when_pad_vaapi_unavailable() {
        let vaapi = vaapi_accel_with_opencl(true);
        let ffmpeg_info = ffmpeg_info_with_filters(&[KnownVideoFilter::PadOpencl]);
        let state = sdr_vaapi_state();

        let input: VideoFilter = PadFilter {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
        }
        .into();

        let result = vaapi.best_filter(&input, &ffmpeg_info, &state);
        assert!(
            matches!(result, VideoFilter::PadOpencl(PadOpencl { .. })),
            "expected PadOpencl when pad_vaapi unavailable, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn best_filter_falls_back_to_software_pad_without_hw_filters() {
        let vaapi = vaapi_accel_with_opencl(false);
        let ffmpeg_info = ffmpeg_info_with_filters(&[]);
        let state = sdr_vaapi_state();

        let input: VideoFilter = PadFilter {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
        }
        .into();

        let result = vaapi.best_filter(&input, &ffmpeg_info, &state);
        assert!(
            matches!(result, VideoFilter::Pad(_)),
            "expected software Pad fallback, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn best_filter_ignores_pad_opencl_without_opencl_capabilities() {
        let vaapi = vaapi_accel_with_opencl(false);
        let ffmpeg_info = ffmpeg_info_with_filters(&[KnownVideoFilter::PadOpencl]);
        let state = sdr_vaapi_state();

        let input: VideoFilter = PadFilter {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
        }
        .into();

        let result = vaapi.best_filter(&input, &ffmpeg_info, &state);
        assert!(
            matches!(result, VideoFilter::Pad(_)),
            "expected software Pad when no OpenCL capabilities, got {:?}",
            result.as_arg()
        );
    }

    #[test]
    fn resolve_inserts_hwmap_for_opencl_pad_on_vaapi() {
        let vaapi = vaapi_accel_with_opencl(true);
        let accel = HardwareAccel::Vaapi(vaapi);
        let ffmpeg_info = ffmpeg_info_with_filters(&[KnownVideoFilter::PadOpencl]);
        let initial_state = sdr_vaapi_state();

        let pad: VideoFilter = PadFilter {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
        }
        .into();

        let mut chain = FilterChain::new(vec![PipelineFilter::Video(pad)]);
        chain.resolve(
            &ffmpeg_info,
            &Some(accel),
            &initial_state,
            &FrameSurface::Vaapi,
            &Some(PixelFormat::Nv12),
        );

        let video_filters: Vec<&VideoFilter> = chain
            .filters
            .iter()
            .filter_map(|f| match f {
                PipelineFilter::Video(vf) => Some(vf),
                _ => None,
            })
            .collect();

        assert!(
            video_filters.len() >= 3,
            "expected at least 3 video filters (hwmap + pad + hwmap), got {}",
            video_filters.len()
        );

        assert!(
            matches!(
                video_filters[0],
                VideoFilter::HwMap(HwMapFilter {
                    from_surface: FrameSurface::Vaapi,
                    to_surface: FrameSurface::OpenCL,
                    reverse: false
                })
            ),
            "first filter should be HwMap from Vaapi to OpenCL"
        );

        assert!(
            matches!(video_filters[1], VideoFilter::PadOpencl(_)),
            "second filter should be PadOpencl"
        );

        assert!(
            matches!(
                video_filters[2],
                VideoFilter::HwMap(HwMapFilter {
                    from_surface: FrameSurface::OpenCL,
                    to_surface: FrameSurface::Vaapi,
                    reverse: true,
                })
            ),
            "third filter should be HwMap from OpenCL back to Vaapi"
        );
    }

    #[test]
    fn resolve_pad_vaapi_does_not_insert_hwmap() {
        let vaapi = vaapi_accel_with_opencl(true);
        let accel = HardwareAccel::Vaapi(vaapi);
        let ffmpeg_info =
            ffmpeg_info_with_filters(&[KnownVideoFilter::PadVaapi, KnownVideoFilter::PadOpencl]);
        let initial_state = sdr_vaapi_state();

        let pad: VideoFilter = PadFilter {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
        }
        .into();

        let mut chain = FilterChain::new(vec![PipelineFilter::Video(pad)]);
        chain.resolve(
            &ffmpeg_info,
            &Some(accel),
            &initial_state,
            &FrameSurface::Vaapi,
            &Some(PixelFormat::Nv12),
        );

        let video_filters: Vec<&VideoFilter> = chain
            .filters
            .iter()
            .filter_map(|f| match f {
                PipelineFilter::Video(vf) => Some(vf),
                _ => None,
            })
            .collect();

        let has_hwmap = video_filters
            .iter()
            .any(|f| matches!(f, VideoFilter::HwMap(_)));
        assert!(
            !has_hwmap,
            "pad_vaapi stays on VAAPI surface, no HwMap needed"
        );

        assert!(
            matches!(video_filters[0], VideoFilter::PadVaapi(_)),
            "first video filter should be PadVaapi, got {:?}",
            video_filters[0].as_arg()
        );
    }

    #[test]
    fn resolve_and_build_produces_correct_filter_complex_for_opencl_pad() {
        let vaapi = vaapi_accel_with_opencl(true);
        let accel = HardwareAccel::Vaapi(vaapi);
        let ffmpeg_info = ffmpeg_info_with_filters(&[KnownVideoFilter::PadOpencl]);
        let initial_state = sdr_vaapi_state();

        let pad: VideoFilter = PadFilter {
            size: Some(FrameSize {
                width: 1920,
                height: 1080,
            }),
        }
        .into();

        let mut chain = FilterChain::new(vec![PipelineFilter::Video(pad)]);
        chain.resolve(
            &ffmpeg_info,
            &Some(accel),
            &initial_state,
            &FrameSurface::Vaapi,
            &Some(PixelFormat::Nv12),
        );
        chain.build("0:a", "0:v", None, None);

        let args = chain.as_arg();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "-filter_complex");

        let filter_complex = &args[1];
        assert!(
            filter_complex.contains("hwmap=derive_device=opencl"),
            "filter_complex should contain hwmap to opencl: {filter_complex}"
        );
        assert!(
            filter_complex.contains("pad_opencl=1920:1080:-1:-1:color=black"),
            "filter_complex should contain pad_opencl: {filter_complex}"
        );
        assert!(
            filter_complex.contains("hwmap=derive_device=vaapi"),
            "filter_complex should contain hwmap back to vaapi: {filter_complex}"
        );

        let expected_order = "hwmap=derive_device=opencl,pad_opencl=";
        assert!(
            filter_complex.contains(expected_order),
            "hwmap to opencl should appear immediately before pad_opencl: {filter_complex}"
        );
    }
}
