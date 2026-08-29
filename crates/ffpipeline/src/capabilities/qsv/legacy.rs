//! Capability probe for legacy Intel Media SDK runtimes (API 1.x).
//!
//! `MFXQueryImplsDescription` exists only in API 2.x runtimes. Gen9 to Gen11
//! GPUs have only a legacy runtime, so the dispatcher reports a hardware
//! implementation with empty decoder, encoder and VPP lists. ffmpeg does not
//! read that tree, so ffmpeg still works on these devices.
//!
//! API 1.x has no bulk query. This module asks about one codec or one pixel
//! format at a time.

use std::collections::{HashMap, HashSet};

use libvpl_sys::*;

use crate::capabilities::qsv::{QsvCapabilities, QsvPixelFormat};
use crate::pipeline::VideoFormat;

const CODECS: &[(VideoFormat, u32)] = &[
    (VideoFormat::Av1, MFX_CODEC_AV1),
    (VideoFormat::H264, MFX_CODEC_AVC),
    (VideoFormat::Hevc, MFX_CODEC_HEVC),
    (VideoFormat::Mpeg2Video, MFX_CODEC_MPEG2),
    (VideoFormat::Vc1, MFX_CODEC_VC1),
    (VideoFormat::Vp8, MFX_CODEC_VP8),
    (VideoFormat::Vp9, MFX_CODEC_VP9),
];

const VPP_FORMATS: &[(u32, u8)] = &[
    (MFX_FOURCC_NV12, 8),
    (MFX_FOURCC_P010, 10),
    (MFX_FOURCC_RGB4, 8),
];

/// A legacy session needs a device before it can answer capability questions. On
/// Linux the caller must supply a VA display, or every query returns
/// `MFX_ERR_UNSUPPORTED`. On Windows the runtime makes its own D3D device, so
/// only the acceleration path changes.
#[cfg(target_os = "linux")]
const IMPL_CANDIDATES: &[mfxIMPL] = &[MFX_IMPL_HARDWARE_ANY | MFX_IMPL_VIA_VAAPI];

#[cfg(not(target_os = "linux"))]
const IMPL_CANDIDATES: &[mfxIMPL] = &[
    MFX_IMPL_HARDWARE_ANY | MFX_IMPL_VIA_D3D11,
    MFX_IMPL_HARDWARE_ANY | MFX_IMPL_VIA_D3D9,
    MFX_IMPL_HARDWARE_ANY,
];

pub(crate) fn probe(vpl: &VplLib) -> Option<QsvCapabilities> {
    #[cfg(target_os = "linux")]
    let display = match display::VaDisplay::open() {
        Some(display) => display,
        None => {
            log::debug!("[qsv] no Intel VA display available for the legacy probe");
            return None;
        }
    };

    for candidate in IMPL_CANDIDATES {
        let Some(session) = Session::open(vpl, *candidate) else {
            log::debug!("[qsv] legacy MFXInit failed for impl 0x{candidate:x}");
            continue;
        };

        #[cfg(target_os = "linux")]
        if !session.set_va_display(display.handle()) {
            continue;
        }

        session.log_identity(*candidate);

        let capabilities = session.probe_capabilities();
        if capabilities.count() > 0 {
            return Some(capabilities);
        }

        log::debug!("[qsv] legacy probe found no codecs using impl 0x{candidate:x}");
    }

    None
}

struct Session<'a> {
    vpl: &'a VplLib,
    handle: mfxSession,
}

impl<'a> Session<'a> {
    fn open(vpl: &'a VplLib, implementation: mfxIMPL) -> Option<Self> {
        let mut handle: mfxSession = std::ptr::null_mut();
        // ask for 1.0 so that any legacy runtime accepts the request; MFXQueryVersion
        // reports the version the runtime really has
        let mut version = mfxVersion { Minor: 0, Major: 1 };
        let status = unsafe { (vpl.MFXInit)(implementation, &mut version, &mut handle) };
        if status != MFX_ERR_NONE || handle.is_null() {
            return None;
        }

        Some(Self { vpl, handle })
    }

    #[cfg(target_os = "linux")]
    fn set_va_display(&self, display: *mut std::ffi::c_void) -> bool {
        let status = unsafe {
            (self.vpl.MFXVideoCORE_SetHandle)(self.handle, MFX_HANDLE_VA_DISPLAY, display)
        };
        if status != MFX_ERR_NONE {
            log::debug!("[qsv] MFXVideoCORE_SetHandle(VA_DISPLAY) failed: {status}");
            return false;
        }

        true
    }

    fn log_identity(&self, requested: mfxIMPL) {
        let mut version = mfxVersion::default();
        let mut implementation: mfxIMPL = 0;
        let mut platform = mfxPlatform::default();
        unsafe {
            (self.vpl.MFXQueryVersion)(self.handle, &mut version);
            (self.vpl.MFXQueryIMPL)(self.handle, &mut implementation);
            // QueryPlatform needs API 1.19. a failure here means the session has no
            // device, and then every capability query reports unsupported
            let status = (self.vpl.MFXVideoCORE_QueryPlatform)(self.handle, &mut platform);
            if status != MFX_ERR_NONE {
                log::debug!("[qsv] legacy MFXVideoCORE_QueryPlatform failed: {status}");
            }
        }

        log::debug!(
            "[qsv] legacy Media SDK session: requested impl 0x{requested:x}, got impl 0x{implementation:x}, API {}.{}, device 0x{:x}",
            version.Major,
            version.Minor,
            platform.DeviceId,
        );
    }

    fn probe_capabilities(&self) -> QsvCapabilities {
        let mut supported_decoders: HashMap<VideoFormat, Vec<u8>> = HashMap::new();
        let mut supported_encoders: HashMap<VideoFormat, Vec<u8>> = HashMap::new();

        for (format, codec_id) in CODECS {
            let decode: Vec<u8> = [8u8, 10]
                .into_iter()
                .filter(|bit_depth| self.can_decode(*codec_id, *bit_depth))
                .collect();
            if !decode.is_empty() {
                supported_decoders.insert(*format, decode);
            }

            let encode: Vec<u8> = [8u8, 10]
                .into_iter()
                .filter(|bit_depth| self.can_encode(*codec_id, *bit_depth))
                .collect();
            if !encode.is_empty() {
                supported_encoders.insert(*format, encode);
            }
        }

        let mut vpp_pixel_formats = HashSet::new();
        for (fourcc, bit_depth) in VPP_FORMATS {
            // VPP accepts rgb4 only as an input, so test both directions
            if self.can_vpp((*fourcc, *bit_depth), (MFX_FOURCC_NV12, 8))
                || self.can_vpp((MFX_FOURCC_NV12, 8), (*fourcc, *bit_depth))
            {
                vpp_pixel_formats.insert(QsvPixelFormat(*fourcc));
            }
        }

        QsvCapabilities {
            supported_decoders,
            supported_encoders,
            vpp_pixel_formats,
        }
    }

    fn can_decode(&self, codec_id: u32, bit_depth: u8) -> bool {
        let mfx = mfxInfoMFX {
            CodecId: codec_id,
            CodecProfile: profile_for(codec_id, bit_depth),
            FrameInfo: frame_info(fourcc_for(bit_depth), bit_depth),
            ..Default::default()
        };

        let mut input = mfxVideoParam::zeroed();
        input.u = codec_union(mfx);
        input.IOPattern = MFX_IOPATTERN_OUT_VIDEO_MEMORY;

        self.query(Query::Decode, codec_id, &mut input)
    }

    fn can_encode(&self, codec_id: u32, bit_depth: u8) -> bool {
        let mfx = mfxInfoMFX {
            CodecId: codec_id,
            CodecProfile: profile_for(codec_id, bit_depth),
            TargetUsage: MFX_TARGETUSAGE_BALANCED,
            RateControlMethod: MFX_RATECONTROL_CQP,
            QPI: 26,
            QPP: 26,
            QPB: 26,
            GopPicSize: 30,
            GopRefDist: 1,
            FrameInfo: frame_info(fourcc_for(bit_depth), bit_depth),
            ..Default::default()
        };

        let mut input = mfxVideoParam::zeroed();
        input.u = codec_union(mfx);
        input.IOPattern = MFX_IOPATTERN_IN_VIDEO_MEMORY;

        self.query(Query::Encode, codec_id, &mut input)
    }

    fn can_vpp(&self, from: (u32, u8), to: (u32, u8)) -> bool {
        let vpp = mfxInfoVPP {
            In: frame_info(from.0, from.1),
            Out: frame_info(to.0, to.1),
            ..Default::default()
        };

        let mut input = mfxVideoParam::zeroed();
        input.u = mfxVideoParamUnion { vpp };
        input.IOPattern = MFX_IOPATTERN_IN_VIDEO_MEMORY | MFX_IOPATTERN_OUT_VIDEO_MEMORY;

        self.query(Query::Vpp, 0, &mut input)
    }

    fn query(&self, kind: Query, codec_id: u32, input: &mut mfxVideoParam) -> bool {
        let mut output = mfxVideoParam::zeroed();

        // ENCODE_Query and DECODE_Query pick the handler from the *output* codec id.
        // a zero id picks the plugin handler. a legacy runtime built with assertions
        // on, such as Debian's libmfxhw64, aborts the process there.
        if codec_id != 0 {
            output.u = codec_union(mfxInfoMFX {
                CodecId: codec_id,
                ..Default::default()
            });
        }

        let query = match kind {
            Query::Decode => self.vpl.MFXVideoDECODE_Query,
            Query::Encode => self.vpl.MFXVideoENCODE_Query,
            Query::Vpp => self.vpl.MFXVideoVPP_Query,
        };

        let status = unsafe { query(self.handle, input, &mut output) };

        // a warning means the runtime changed the parameters but can still use the
        // hardware. partial acceleration means it falls back to software, which is
        // not a capability
        status >= MFX_ERR_NONE && status != MFX_WRN_PARTIAL_ACCELERATION
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.vpl.MFXClose)(self.handle);
        }
    }
}

#[derive(Copy, Clone)]
enum Query {
    Decode,
    Encode,
    Vpp,
}

/// `mfxInfoMFX` is smaller than `mfxInfoVPP`, the other arm of the union. write the
/// larger arm first so that the extra bytes stay zero.
fn codec_union(mfx: mfxInfoMFX) -> mfxVideoParamUnion {
    let mut union = mfxVideoParamUnion {
        vpp: mfxInfoVPP::default(),
    };
    union.mfx = mfx;
    union
}

fn fourcc_for(bit_depth: u8) -> u32 {
    if bit_depth == 10 {
        MFX_FOURCC_P010
    } else {
        MFX_FOURCC_NV12
    }
}

fn frame_info(fourcc: u32, bit_depth: u8) -> mfxFrameInfo {
    mfxFrameInfo {
        FourCC: fourcc,
        ChromaFormat: MFX_CHROMAFORMAT_YUV420,
        BitDepthLuma: u16::from(bit_depth),
        BitDepthChroma: u16::from(bit_depth),
        Shift: u16::from(bit_depth == 10),
        Width: 1920,
        Height: 1088,
        CropW: 1920,
        CropH: 1080,
        FrameRateExtN: 30000,
        FrameRateExtD: 1001,
        PicStruct: MFX_PICSTRUCT_PROGRESSIVE,
        ..Default::default()
    }
}

/// The profile that pins a query to 10-bit. AV1 Main covers both depths, so the bit
/// depth in `mfxFrameInfo` separates them there.
fn profile_for(codec_id: u32, bit_depth: u8) -> u16 {
    if bit_depth != 10 {
        return 0;
    }

    let profile = match codec_id {
        id if id == MFX_CODEC_AVC => MFX_PROFILE_AVC_HIGH10,
        id if id == MFX_CODEC_HEVC => MFX_PROFILE_HEVC_MAIN10,
        id if id == MFX_CODEC_VP9 => MFX_PROFILE_VP9_2,
        _ => 0,
    };

    profile as u16
}

#[cfg(target_os = "linux")]
mod display {
    use std::ffi::{CStr, c_void};
    use std::fs::File;
    use std::os::unix::io::AsRawFd;

    use libva_sys::{VA_STATUS_SUCCESS, VADisplay, VaLib};

    /// A VA display on an Intel render node. it must stay open for as long as the
    /// Media SDK session that uses it.
    pub(super) struct VaDisplay {
        va: VaLib,
        _device: File,
        display: VADisplay,
    }

    impl VaDisplay {
        pub(super) fn open() -> Option<Self> {
            let va = VaLib::load().ok()?;

            for node in 128..136 {
                let path = format!("/dev/dri/renderD{node}");
                let Ok(device) = File::options().read(true).write(true).open(&path) else {
                    continue;
                };

                let display = unsafe { (va.vaGetDisplayDRM)(device.as_raw_fd()) };
                if display.is_null() {
                    continue;
                }

                let mut major = 0i32;
                let mut minor = 0i32;
                if unsafe { (va.vaInitialize)(display, &mut major, &mut minor) }
                    != VA_STATUS_SUCCESS
                {
                    continue;
                }

                let vendor = unsafe {
                    let ptr = (va.vaQueryVendorString)(display);
                    if ptr.is_null() {
                        String::new()
                    } else {
                        CStr::from_ptr(ptr).to_string_lossy().into_owned()
                    }
                };

                // a machine can have more than one render node, but Media SDK
                // drives only Intel devices
                if !vendor.contains("Intel") {
                    unsafe { (va.vaTerminate)(display) };
                    continue;
                }

                log::debug!("[qsv] legacy probe using {path} ({vendor})");

                return Some(Self {
                    va,
                    _device: device,
                    display,
                });
            }

            None
        }

        pub(super) fn handle(&self) -> *mut c_void {
            self.display
        }
    }

    impl Drop for VaDisplay {
        fn drop(&mut self) {
            unsafe {
                (self.va.vaTerminate)(self.display);
            }
        }
    }
}
