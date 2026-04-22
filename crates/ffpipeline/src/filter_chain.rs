use crate::ArgVec;
use crate::audio_filter::AudioFilter;
use crate::ffmpeg_info::FfmpegInfo;
use crate::hw_accel::{HardwareAccel, HwAccel};
use crate::pipeline::{FrameState, FrameSurface, PixelFormat};
use crate::video_filter::VideoFilter;

#[derive(Clone)]
pub enum PipelineFilter {
    Audio(AudioFilter),
    Video(VideoFilter),
}

#[derive(Clone)]
pub(crate) struct FilterChain {
    pub(crate) filters: Vec<PipelineFilter>,
    audio_label: String,
    video_label: String,
    complex_filter: String,
}

impl FilterChain {
    pub(crate) fn new(filters: Vec<PipelineFilter>) -> FilterChain {
        FilterChain {
            filters,
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

        for filter in &self.filters {
            match filter {
                PipelineFilter::Video(video_filter) => {
                    let mut best = match accel {
                        Some(a) => a.best_filter(video_filter, ffmpeg_info, &current_state),
                        _ => video_filter.clone(),
                    };

                    if let Some(required) = best.required_surface()
                        && current_state.surface != required
                    {
                        if required == FrameSurface::System {
                            let target_pixel_format = match current_state.pixel_format.bit_depth() {
                                10 => PixelFormat::P010le,
                                _ => PixelFormat::Nv12,
                            };

                            let download = VideoFilter::HwDownload {
                                target_pixel_format,
                            };
                            download.apply_to(&mut current_state);
                            resolved.push(PipelineFilter::Video(download));
                        } else if current_state.surface == FrameSurface::System {
                            let is_format_supported = match accel {
                                Some(a) => a.supports_pixel_format(&current_state.pixel_format),
                                None => true,
                            };

                            if is_format_supported {
                                let upload = VideoFilter::HwUpload {
                                    target_surface: required.clone(),
                                    source_format: current_state.pixel_format.clone(),
                                };
                                upload.apply_to(&mut current_state);
                                resolved.push(PipelineFilter::Video(upload))
                            } else {
                                let can_convert_down = current_state.pixel_format.bit_depth() == 10
                                    && encoder_pixel_format
                                        .as_ref()
                                        .is_some_and(|pf| pf.bit_depth() == 8);

                                if can_convert_down {
                                    let format = VideoFilter::Format {
                                        format: PixelFormat::Nv12,
                                    };
                                    format.apply_to(&mut current_state);
                                    resolved.push(PipelineFilter::Video(format));

                                    let upload = VideoFilter::HwUpload {
                                        target_surface: required,
                                        source_format: current_state.pixel_format.clone(),
                                    };
                                    upload.apply_to(&mut current_state);
                                    resolved.push(PipelineFilter::Video(upload));
                                } else {
                                    best = video_filter.clone();
                                }
                            }
                        } else if let Some(map) = accel
                            .as_ref()
                            .and_then(|a| a.hw_map_filter(&current_state.surface, &required))
                        {
                            map.apply_to(&mut current_state);
                            resolved.push(PipelineFilter::Video(map));
                        }
                    }

                    best.apply_to(&mut current_state);
                    resolved.push(PipelineFilter::Video(best));
                }
                PipelineFilter::Audio(_) => {
                    // not sure if we should actually apply audio filters to the state
                    // since they are separate in the real filter graph
                    //audio_filter.apply_to(&mut current_state);
                    resolved.push(filter.clone());
                }
            }
        }

        if current_state.surface != *encoder_surface {
            log::debug!(
                "current surface {:?} doesn't match encoder {:?}",
                current_state.surface,
                *encoder_surface
            );

            if *encoder_surface == FrameSurface::System {
                let target_pixel_format = match current_state.pixel_format.bit_depth() {
                    10 => PixelFormat::P010le,
                    _ => PixelFormat::Nv12,
                };

                let download = VideoFilter::HwDownload {
                    target_pixel_format,
                };
                download.apply_to(&mut current_state);
                resolved.push(PipelineFilter::Video(download));
            } else if current_state.surface == FrameSurface::System {
                let upload = VideoFilter::HwUpload {
                    target_surface: encoder_surface.clone(),
                    source_format: current_state.pixel_format.clone(),
                };
                upload.apply_to(&mut current_state);
                resolved.push(PipelineFilter::Video(upload))
            } else if let Some(map) = accel
                .as_ref()
                .and_then(|a| a.hw_map_filter(&current_state.surface, encoder_surface))
            {
                map.apply_to(&mut current_state);
                resolved.push(PipelineFilter::Video(map));
            }
        }

        if let Some(pixel_format) = encoder_pixel_format
            && current_state.pixel_format != *pixel_format
        {
            log::debug!(
                "current pixel format {:?} doesn't match encoder {:?}",
                current_state.pixel_format,
                *pixel_format
            );

            match (&current_state.surface, accel) {
                (FrameSurface::System, _) => {
                    let format = VideoFilter::Format {
                        format: pixel_format.to_owned(),
                    };
                    format.apply_to(&mut current_state);
                    resolved.push(PipelineFilter::Video(format))
                }
                (_, Some(a)) => {
                    if let Some(f) = a.format_filter(pixel_format) {
                        f.apply_to(&mut current_state);
                        resolved.push(PipelineFilter::Video(f));
                    }
                }
                _ => {}
            }
        }

        self.filters = resolved;
    }

    pub(crate) fn optimize(&mut self) {
        // swap software scale before software tone map to reduce
        // the amount of data that needs to be tone mapped
        if let Some(tonemap_index) = self
            .filters
            .iter()
            .position(|f| matches!(f, PipelineFilter::Video(VideoFilter::ToneMap { .. })))
            && let Some(PipelineFilter::Video(VideoFilter::Scale { .. })) =
                self.filters.get(tonemap_index + 1)
        {
            log::debug!("swapping software scale filter before software tonemap filter");
            self.filters.swap(tonemap_index, tonemap_index + 1);
        }
    }

    pub(crate) fn build(&mut self, audio_label: &str, video_label: &str) {
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
            .filter(|f| matches!(f, PipelineFilter::Video(_)))
            .count();

        if video_filter_count > 0 {
            let mut filter_chain = String::new();

            filter_chain.push_str(&format!("[{}]", self.video_label));

            let mut video_chain: Vec<String> = Vec::new();

            for filter in self.filters.iter() {
                if let PipelineFilter::Video(video_filter) = filter
                    && let Some(arg) = video_filter.as_arg()
                {
                    video_chain.push(arg);
                }
            }

            filter_chain.push_str(&video_chain.join(","));

            self.video_label = String::from("[v]");
            filter_chain.push_str(&self.video_label);

            filter_chains.push(filter_chain)
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
    use std::collections::HashSet;

    use super::*;
    use crate::accel::opencl::TonemapOpencl;
    use crate::accel::vaapi::{Vaapi, VaapiDriver};
    use crate::capabilities::vaapi::VaapiCapabilities;
    use crate::frame_size::FrameSize;
    use crate::hw_accel::HardwareAccel;

    fn vaapi_accel() -> HardwareAccel {
        HardwareAccel::Vaapi(Vaapi {
            device: String::from("/dev/dri/renderD128"),
            driver: VaapiDriver::Ihd,
            capabilities: VaapiCapabilities {
                vendor: String::from("test"),
                supported: HashSet::new(),
                vpp_pixel_formats: HashSet::new(),
            },
            needs_opencl_device: true,
        })
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

        let tonemap = VideoFilter::Hardware(Box::new(TonemapOpencl {
            algorithm: Some(String::from("hable")),
            output_format: PixelFormat::Nv12,
        }));

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
                VideoFilter::HwMap {
                    from_surface: FrameSurface::Vaapi,
                    to_surface: FrameSurface::OpenCL,
                    reverse: false
                }
            ),
            "first filter should be HwMap from Vaapi to OpenCL"
        );

        assert!(
            matches!(video_filters[1], VideoFilter::Hardware(_)),
            "second filter should be the tonemap Hardware filter"
        );

        assert!(
            matches!(
                video_filters[2],
                VideoFilter::HwMap {
                    from_surface: FrameSurface::OpenCL,
                    to_surface: FrameSurface::Vaapi,
                    reverse: true,
                }
            ),
            "third filter should be HwMap from OpenCL back to Vaapi"
        );
    }

    #[test]
    fn resolve_and_build_produces_correct_filter_complex_for_opencl_tonemap() {
        let accel = vaapi_accel();
        let initial_state = hdr_vaapi_state();
        let ffmpeg_info = FfmpegInfo::default();

        let tonemap = VideoFilter::Hardware(Box::new(TonemapOpencl {
            algorithm: Some(String::from("hable")),
            output_format: PixelFormat::Nv12,
        }));

        let mut chain = FilterChain::new(vec![PipelineFilter::Video(tonemap)]);

        chain.resolve(
            &ffmpeg_info,
            &Some(accel),
            &initial_state,
            &FrameSurface::Vaapi,
            &Some(PixelFormat::Nv12),
        );
        chain.build("0:v", "0:v");

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

        let format_filter = VideoFilter::Format {
            format: PixelFormat::Nv12,
        };

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
            .any(|f| matches!(f, PipelineFilter::Video(VideoFilter::HwMap { .. })));

        assert!(
            !has_hwmap,
            "no HwMap should be inserted when no hw-to-hw transition is needed"
        );
    }
}
