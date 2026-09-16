use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::ptr;

use libamf_sys::{
    AMF_ACCEL_GPU, AMF_ACCEL_HARDWARE, AMF_ACCEL_NOT_SUPPORTED, AMF_ACCEL_SOFTWARE,
    AMF_FULL_VERSION, AMF_OK, AMF_SURFACE_NV12, AMF_SURFACE_P010, AMF_VARIANT_INT64,
    AMF_VIDEO_ENCODER_AV1_CAP_BFRAMES, AMF_VIDEO_ENCODER_AV1_CAP_MAX_LEVEL,
    AMF_VIDEO_ENCODER_AV1_CAP_MAX_PROFILE, AMF_VIDEO_ENCODER_AV1_COLOR_BIT_DEPTH,
    AMF_VIDEO_ENCODER_AV1_PROFILE, AMF_VIDEO_ENCODER_AV1_PROFILE_MAIN,
    AMF_VIDEO_ENCODER_CAP_BFRAMES, AMF_VIDEO_ENCODER_CAP_MAX_LEVEL,
    AMF_VIDEO_ENCODER_CAP_MAX_PROFILE, AMF_VIDEO_ENCODER_HEVC_CAP_MAX_LEVEL,
    AMF_VIDEO_ENCODER_HEVC_CAP_MAX_PROFILE, AMF_VIDEO_ENCODER_HEVC_COLOR_BIT_DEPTH,
    AMF_VIDEO_ENCODER_HEVC_PROFILE, AMF_VIDEO_ENCODER_HEVC_PROFILE_MAIN_10, AMFCaps, AMFComponent,
    AMFContext, AMFContext1, AMFFactory, AMFIOCaps, AMFVariantStruct, AMFVideoConverter,
    AMFVideoDecoderHW_AV1, AMFVideoDecoderHW_H265_HEVC, AMFVideoDecoderHW_H265_MAIN10,
    AMFVideoDecoderHW_VP9, AMFVideoDecoderHW_VP9_10BIT, AMFVideoDecoderUVD_H264_AVC,
    AMFVideoDecoderUVD_MPEG2, AMFVideoDecoderUVD_VC1, AMFVideoEncoder_AV1, AMFVideoEncoder_HEVC,
    AMFVideoEncoderVCE_AVC, AmfInterface, AmfLib, IID_AMFContext1, amf_memory_type_name,
    amf_result_name, amf_surface_format_name, amf_version_parts, wide,
};

use crate::capabilities::amf::{
    AmfAdapter, AmfCapabilities, AmfDevice, AmfDeviceTarget, AmfEncoderCapability, AmfSurfaceFormat,
};
use crate::error::FFPipelineError;
use crate::pipeline::VideoFormat;

/// A decoder's output format list is not a bit-depth signal: the H.264 decoder lists
/// P010 but corrupts 10-bit streams. ffmpeg creates a bit-depth-specific component on
/// drivers without bitness detection, so that component existing is the signal.
/// AV1 Main covers 8 and 10-bit.
const DECODERS: &[(VideoFormat, &str, &[u8])] = &[
    (VideoFormat::Mpeg2Video, AMFVideoDecoderUVD_MPEG2, &[8]),
    (VideoFormat::Vc1, AMFVideoDecoderUVD_VC1, &[8]),
    (VideoFormat::H264, AMFVideoDecoderUVD_H264_AVC, &[8]),
    (VideoFormat::Hevc, AMFVideoDecoderHW_H265_HEVC, &[8]),
    (VideoFormat::Hevc, AMFVideoDecoderHW_H265_MAIN10, &[10]),
    (VideoFormat::Vp9, AMFVideoDecoderHW_VP9, &[8]),
    (VideoFormat::Vp9, AMFVideoDecoderHW_VP9_10BIT, &[10]),
    (VideoFormat::Av1, AMFVideoDecoderHW_AV1, &[8, 10]),
];

struct EncoderProps {
    max_profile: &'static str,
    max_level: &'static str,
    /// HEVC has no B-frame capability property in the SDK headers
    b_frames: Option<&'static str>,
    ten_bit: Option<TenBitInit>,
}

struct TenBitInit {
    profile: &'static str,
    profile_value: i64,
    color_bit_depth: &'static str,
}

const ENCODERS: &[(VideoFormat, &str, EncoderProps)] = &[
    (
        VideoFormat::H264,
        AMFVideoEncoderVCE_AVC,
        EncoderProps {
            max_profile: AMF_VIDEO_ENCODER_CAP_MAX_PROFILE,
            max_level: AMF_VIDEO_ENCODER_CAP_MAX_LEVEL,
            b_frames: Some(AMF_VIDEO_ENCODER_CAP_BFRAMES),
            ten_bit: None,
        },
    ),
    (
        VideoFormat::Hevc,
        AMFVideoEncoder_HEVC,
        EncoderProps {
            max_profile: AMF_VIDEO_ENCODER_HEVC_CAP_MAX_PROFILE,
            max_level: AMF_VIDEO_ENCODER_HEVC_CAP_MAX_LEVEL,
            b_frames: None,
            ten_bit: Some(TenBitInit {
                profile: AMF_VIDEO_ENCODER_HEVC_PROFILE,
                profile_value: AMF_VIDEO_ENCODER_HEVC_PROFILE_MAIN_10,
                color_bit_depth: AMF_VIDEO_ENCODER_HEVC_COLOR_BIT_DEPTH,
            }),
        },
    ),
    (
        VideoFormat::Av1,
        AMFVideoEncoder_AV1,
        EncoderProps {
            max_profile: AMF_VIDEO_ENCODER_AV1_CAP_MAX_PROFILE,
            max_level: AMF_VIDEO_ENCODER_AV1_CAP_MAX_LEVEL,
            b_frames: Some(AMF_VIDEO_ENCODER_AV1_CAP_BFRAMES),
            ten_bit: Some(TenBitInit {
                profile: AMF_VIDEO_ENCODER_AV1_PROFILE,
                profile_value: AMF_VIDEO_ENCODER_AV1_PROFILE_MAIN,
                color_bit_depth: AMF_VIDEO_ENCODER_AV1_COLOR_BIT_DEPTH,
            }),
        },
    ),
];

struct Owned<T: AmfInterface>(*mut T);

impl<T: AmfInterface> Drop for Owned<T> {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { T::release(self.0) };
        }
    }
}

/// Terminate must run before Release
struct Context(*mut AMFContext);

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            ((*(*self.0).pVtbl).Terminate)(self.0);
            AMFContext::release(self.0);
        }
    }
}

fn error(message: String) -> FFPipelineError {
    FFPipelineError::AmfCapabilitiesError(message)
}

struct Backing {
    adapter: AmfAdapter,
    #[cfg(target_os = "windows")]
    device: libd3d11_sys::D3d11Device,
}

impl Backing {
    #[cfg(target_os = "windows")]
    fn d3d11_device(&self) -> *mut c_void {
        self.device.as_ptr()
    }
}

#[cfg(target_os = "windows")]
fn select_backing(target: AmfDeviceTarget) -> Result<Option<Backing>, FFPipelineError> {
    Ok(super::adapter::select(target)?.map(|selected| Backing {
        adapter: selected.info,
        device: selected.device,
    }))
}

/// ffmpeg cannot derive AMF from a Vulkan device, so a choice made here could not be
/// passed on. The runtime picks on both sides instead.
#[cfg(not(target_os = "windows"))]
fn select_backing(target: AmfDeviceTarget) -> Result<Option<Backing>, FFPipelineError> {
    if let AmfDeviceTarget::Adapter(index) = target {
        log::warn!("[amf] amf_device {index} is only supported on Windows; ignoring");
    }
    Ok(None)
}

impl AmfCapabilities {
    pub fn probe_with(target: AmfDeviceTarget) -> Result<AmfCapabilities, FFPipelineError> {
        let amf =
            AmfLib::load().map_err(|e| error(format!("failed to load the AMF runtime: {e}")))?;

        // kept in this frame so the D3D11 device outlives the AMF context built on it
        let backing = select_backing(target)?;

        unsafe { probe_amf(&amf, backing.as_ref()) }
    }
}

unsafe fn probe_amf(
    amf: &AmfLib,
    backing: Option<&Backing>,
) -> Result<AmfCapabilities, FFPipelineError> {
    unsafe {
        let mut runtime = 0u64;
        let runtime_version = match (amf.AMFQueryVersion)(&mut runtime) {
            AMF_OK => Some(runtime),
            result => {
                log::trace!("[amf] AMFQueryVersion failed: {}", amf_result_name(result));
                None
            }
        };
        if let Some(version) = runtime_version {
            let (major, minor, release, build) = amf_version_parts(version);
            log::trace!("[amf] runtime version {major}.{minor}.{release}.{build}");
        }

        // never ask the runtime for a newer API than it reports
        let requested = runtime_version.map_or(AMF_FULL_VERSION, |v| v.min(AMF_FULL_VERSION));
        let mut factory: *mut AMFFactory = ptr::null_mut();
        let result = (amf.AMFInit)(requested, &mut factory);
        if result != AMF_OK || factory.is_null() {
            return Err(error(format!(
                "AMFInit failed: {}",
                amf_result_name(result)
            )));
        }

        let mut context: *mut AMFContext = ptr::null_mut();
        let result = ((*(*factory).pVtbl).CreateContext)(factory, &mut context);
        if result != AMF_OK || context.is_null() {
            return Err(error(format!(
                "CreateContext failed: {}",
                amf_result_name(result)
            )));
        }
        let context = Context(context);

        let device = init_device(context.0, backing)?;
        match backing {
            Some(backing) => log::trace!(
                "[amf] context initialized on {} (adapter {}: \"{}\")",
                device.name(),
                backing.adapter.index,
                backing.adapter.description
            ),
            None => log::trace!("[amf] context initialized on {}", device.name()),
        }

        let mut supported_decoders: HashMap<VideoFormat, Vec<u8>> = HashMap::new();
        for (format, id, bit_depths) in DECODERS {
            let depths = probe_decoder(factory, context.0, id, bit_depths);
            if !depths.is_empty() {
                supported_decoders
                    .entry(*format)
                    .or_default()
                    .extend(depths);
            }
        }

        let mut supported_encoders = HashMap::new();
        for (format, id, props) in ENCODERS {
            if let Some(capability) = probe_encoder(factory, context.0, id, props) {
                supported_encoders.insert(*format, capability);
            }
        }

        let (vpp_input_formats, vpp_output_formats) = probe_converter(factory, context.0);

        Ok(AmfCapabilities {
            supported_decoders,
            supported_encoders,
            vpp_input_formats,
            vpp_output_formats,
            runtime_version: runtime_version.map(amf_version_parts),
            device: Some(device),
            adapter: backing.map(|b| b.adapter.clone()),
        })
    }
}

/// Same backend order as ffmpeg's hwcontext_amf: DX11, DX9, then Vulkan. With a backing
/// D3D11 device there is no fallback, because another backend would land on another GPU.
unsafe fn init_device(
    context: *mut AMFContext,
    backing: Option<&Backing>,
) -> Result<AmfDevice, FFPipelineError> {
    let mut failures = Vec::new();

    #[cfg(target_os = "windows")]
    unsafe {
        let d3d11_device = backing.map_or(ptr::null_mut(), Backing::d3d11_device);
        let result = ((*(*context).pVtbl).InitDX11)(context, d3d11_device, libamf_sys::AMF_DX11_1);
        if result == AMF_OK {
            return Ok(AmfDevice::Dx11);
        }
        if let Some(backing) = backing {
            return Err(error(format!(
                "InitDX11 on adapter {} (\"{}\") failed: {}",
                backing.adapter.index,
                backing.adapter.description,
                amf_result_name(result)
            )));
        }
        failures.push(format!("InitDX11: {}", amf_result_name(result)));

        let result = ((*(*context).pVtbl).InitDX9)(context, ptr::null_mut());
        if result == AMF_OK {
            return Ok(AmfDevice::Dx9);
        }
        failures.push(format!("InitDX9: {}", amf_result_name(result)));
    }

    #[cfg(not(target_os = "windows"))]
    let _ = backing;

    unsafe {
        let mut context1: *mut AMFContext1 = ptr::null_mut();
        let result = ((*(*context).pVtbl).QueryInterface)(
            context,
            &IID_AMFContext1,
            (&raw mut context1).cast::<*mut c_void>(),
        );
        if result == AMF_OK && !context1.is_null() {
            let context1 = Owned(context1);
            let result = ((*(*context1.0).pVtbl).InitVulkan)(context1.0, ptr::null_mut());
            if result == AMF_OK {
                return Ok(AmfDevice::Vulkan);
            }
            failures.push(format!("InitVulkan: {}", amf_result_name(result)));
        } else {
            failures.push(format!(
                "AMFContext1 (Vulkan) unavailable: {}",
                amf_result_name(result)
            ));
        }
    }

    Err(error(format!("no usable device ({})", failures.join("; "))))
}

unsafe fn create_component(
    factory: *mut AMFFactory,
    context: *mut AMFContext,
    id: &str,
) -> Option<Owned<AMFComponent>> {
    unsafe {
        let wide_id = wide(id);
        let mut component: *mut AMFComponent = ptr::null_mut();
        let result = ((*(*factory).pVtbl).CreateComponent)(
            factory,
            context,
            wide_id.as_ptr(),
            &mut component,
        );
        if result != AMF_OK || component.is_null() {
            log::trace!(
                "[amf] {id}: CreateComponent failed: {}",
                amf_result_name(result)
            );
            return None;
        }
        Some(Owned(component))
    }
}

unsafe fn get_caps(id: &str, component: &Owned<AMFComponent>) -> Option<Owned<AMFCaps>> {
    unsafe {
        let mut caps: *mut AMFCaps = ptr::null_mut();
        let result = ((*(*component.0).pVtbl).GetCaps)(component.0, &mut caps);
        if result != AMF_OK || caps.is_null() {
            log::trace!("[amf] {id}: GetCaps failed: {}", amf_result_name(result));
            return None;
        }
        let caps = Owned(caps);

        let accel = ((*(*caps.0).pVtbl).GetAccelerationType)(caps.0);
        let accel_name = match accel {
            AMF_ACCEL_NOT_SUPPORTED => "not supported",
            AMF_ACCEL_HARDWARE => "hardware",
            AMF_ACCEL_GPU => "gpu",
            AMF_ACCEL_SOFTWARE => "software",
            _ => "unknown",
        };
        log::trace!("[amf] {id}: acceleration {accel_name}");
        if accel == AMF_ACCEL_NOT_SUPPORTED {
            return None;
        }

        Some(caps)
    }
}

#[derive(Clone, Copy)]
enum Side {
    Input,
    Output,
}

unsafe fn io_formats(id: &str, caps: &Owned<AMFCaps>, side: Side) -> HashSet<AmfSurfaceFormat> {
    unsafe {
        let mut io: *mut AMFIOCaps = ptr::null_mut();
        let (label, result) = match side {
            Side::Input => ("input", ((*(*caps.0).pVtbl).GetInputCaps)(caps.0, &mut io)),
            Side::Output => (
                "output",
                ((*(*caps.0).pVtbl).GetOutputCaps)(caps.0, &mut io),
            ),
        };
        if result != AMF_OK || io.is_null() {
            log::trace!(
                "[amf] {id}: {label} caps unavailable: {}",
                amf_result_name(result)
            );
            return HashSet::new();
        }
        let io = Owned(io);

        let (mut min_w, mut max_w, mut min_h, mut max_h) = (0, 0, 0, 0);
        ((*(*io.0).pVtbl).GetWidthRange)(io.0, &mut min_w, &mut max_w);
        ((*(*io.0).pVtbl).GetHeightRange)(io.0, &mut min_h, &mut max_h);

        let mut formats = HashSet::new();
        let mut names = Vec::new();
        for i in 0..((*(*io.0).pVtbl).GetNumOfFormats)(io.0) {
            let mut format = 0;
            let mut native = 0;
            if ((*(*io.0).pVtbl).GetFormatAt)(io.0, i, &mut format, &mut native) == AMF_OK {
                formats.insert(AmfSurfaceFormat(format));
                let name = amf_surface_format_name(format);
                names.push(if native != 0 {
                    format!("{name}*")
                } else {
                    name
                });
            }
        }

        let mut memory_types = Vec::new();
        for i in 0..((*(*io.0).pVtbl).GetNumOfMemoryTypes)(io.0) {
            let mut memory_type = 0;
            let mut native = 0;
            if ((*(*io.0).pVtbl).GetMemoryTypeAt)(io.0, i, &mut memory_type, &mut native) == AMF_OK
            {
                let name = amf_memory_type_name(memory_type);
                memory_types.push(if native != 0 {
                    format!("{name}*")
                } else {
                    name
                });
            }
        }

        log::trace!(
            "[amf] {id}: {label} {min_w}x{min_h}..{max_w}x{max_h} formats [{}] memory [{}]",
            names.join(" "),
            memory_types.join(" "),
        );

        formats
    }
}

unsafe fn caps_property(caps: &Owned<AMFCaps>, name: &str) -> Option<AMFVariantStruct> {
    unsafe {
        let wide_name = wide(name);
        let mut value = AMFVariantStruct::empty();
        let result = ((*(*caps.0).pVtbl).GetProperty)(caps.0, wide_name.as_ptr(), &mut value);
        (result == AMF_OK).then_some(value)
    }
}

unsafe fn set_int64_property(component: &Owned<AMFComponent>, name: &str, value: i64) -> bool {
    unsafe {
        let wide_name = wide(name);
        let mut variant = AMFVariantStruct::empty();
        variant.r#type = AMF_VARIANT_INT64;
        variant.value.int64Value = value;
        let result =
            ((*(*component.0).pVtbl).SetProperty)(component.0, wide_name.as_ptr(), variant);
        if result != AMF_OK {
            log::trace!(
                "[amf] SetProperty({name}={value}) failed: {}",
                amf_result_name(result)
            );
        }
        result == AMF_OK
    }
}

/// ffmpeg exchanges NV12 for 8-bit and P010 for 10-bit with the amf codecs
fn surface_for_bit_depth(bit_depth: u8) -> Option<AmfSurfaceFormat> {
    match bit_depth {
        8 => Some(AmfSurfaceFormat(AMF_SURFACE_NV12)),
        10 => Some(AmfSurfaceFormat(AMF_SURFACE_P010)),
        _ => None,
    }
}

fn bit_depths(formats: &HashSet<AmfSurfaceFormat>, candidates: &[u8]) -> Vec<u8> {
    candidates
        .iter()
        .copied()
        .filter(|bd| surface_for_bit_depth(*bd).is_some_and(|s| formats.contains(&s)))
        .collect()
}

unsafe fn probe_decoder(
    factory: *mut AMFFactory,
    context: *mut AMFContext,
    id: &str,
    candidates: &[u8],
) -> Vec<u8> {
    unsafe {
        let Some(component) = create_component(factory, context, id) else {
            return Vec::new();
        };
        let Some(caps) = get_caps(id, &component) else {
            return Vec::new();
        };
        bit_depths(&io_formats(id, &caps, Side::Output), candidates)
    }
}

/// Caps tables are per driver, not per ASIC: Polaris reports P010 input and a Main 10
/// max profile for HEVC but cannot encode 10-bit. Init is the only call the runtime
/// answers honestly. It runs on a fresh component because a reused one can keep state
/// from the previous attempt.
unsafe fn trial_init(
    factory: *mut AMFFactory,
    context: *mut AMFContext,
    id: &str,
    bit_depth: u8,
    ten_bit: Option<&TenBitInit>,
) -> bool {
    unsafe {
        let Some(component) = create_component(factory, context, id) else {
            return false;
        };
        let surface = match bit_depth {
            8 => AMF_SURFACE_NV12,
            10 => {
                let Some(ten_bit) = ten_bit else {
                    return false;
                };
                let ok = set_int64_property(&component, ten_bit.profile, ten_bit.profile_value)
                    && set_int64_property(&component, ten_bit.color_bit_depth, 10);
                if !ok {
                    return false;
                }
                AMF_SURFACE_P010
            }
            _ => return false,
        };
        let result = ((*(*component.0).pVtbl).Init)(component.0, surface, 1920, 1080);
        log::trace!(
            "[amf] {id}: {bit_depth}-bit trial Init({}): {}",
            amf_surface_format_name(surface),
            amf_result_name(result)
        );
        ((*(*component.0).pVtbl).Terminate)(component.0);
        result == AMF_OK
    }
}

unsafe fn probe_encoder(
    factory: *mut AMFFactory,
    context: *mut AMFContext,
    id: &str,
    props: &EncoderProps,
) -> Option<AmfEncoderCapability> {
    unsafe {
        let component = create_component(factory, context, id)?;
        let caps = get_caps(id, &component)?;
        let candidates = bit_depths(&io_formats(id, &caps, Side::Input), &[8, 10]);
        if candidates.is_empty() {
            return None;
        }

        let max_profile = caps_property(&caps, props.max_profile).and_then(|v| v.as_int64());
        let max_level = caps_property(&caps, props.max_level).and_then(|v| v.as_int64());
        let b_frames = props
            .b_frames
            .and_then(|name| caps_property(&caps, name))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        log::trace!(
            "[amf] {id}: max profile {max_profile:?}, max level {max_level:?}, b-frames {b_frames}"
        );

        let depths: Vec<u8> = candidates
            .into_iter()
            .filter(|bd| trial_init(factory, context, id, *bd, props.ten_bit.as_ref()))
            .collect();
        if depths.is_empty() {
            return None;
        }

        Some(AmfEncoderCapability {
            bit_depths: depths,
            b_frames,
            max_profile,
            max_level,
        })
    }
}

unsafe fn probe_converter(
    factory: *mut AMFFactory,
    context: *mut AMFContext,
) -> (HashSet<AmfSurfaceFormat>, HashSet<AmfSurfaceFormat>) {
    unsafe {
        let id = AMFVideoConverter;
        let Some(component) = create_component(factory, context, id) else {
            return (HashSet::new(), HashSet::new());
        };
        let Some(caps) = get_caps(id, &component) else {
            return (HashSet::new(), HashSet::new());
        };
        (
            io_formats(id, &caps, Side::Input),
            io_formats(id, &caps, Side::Output),
        )
    }
}
