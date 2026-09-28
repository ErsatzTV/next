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
use std::fmt::Write;

use libvpl_sys::*;

use crate::capabilities::qsv::{QsvCapabilities, QsvFourCC};
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

/// No rgb4: pad runs on decoded video.
const COMPOSITE_FORMATS: &[(u32, u8)] = &[(MFX_FOURCC_NV12, 8), (MFX_FOURCC_P010, 10)];

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
    first_session(vpl, |session, candidate| {
        let capabilities = session.probe_capabilities();
        if capabilities.count() > 0 {
            return Some(capabilities);
        }

        log::debug!("[qsv] legacy probe found no codecs using impl 0x{candidate:x}");
        None
    })
}

/// Raw `MFXVideoVPP_Query` statuses for the queries the probe makes, plus a few it
/// does not. API 2.x runtimes still accept the 1.x entry points through the
/// dispatcher, so this runs on both kinds of runtime.
pub(crate) fn dump(vpl: &VplLib, out: &mut String) {
    let _ = writeln!(out, "== MFXVideoVPP_Query (API 1.x session) ==");
    let opened = first_session(vpl, |session, candidate| {
        session.dump(candidate, out);
        Some(())
    });
    if opened.is_none() {
        let _ = writeln!(out, "no session could be opened");
    }
}

/// Opens a session on each implementation candidate in turn, until `visit` returns
/// `Some`.
fn first_session<T>(
    vpl: &VplLib,
    mut visit: impl FnMut(&Session, mfxIMPL) -> Option<T>,
) -> Option<T> {
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

        if let Some(result) = visit(&session, *candidate) {
            return Some(result);
        }
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

    fn identity(&self, requested: mfxIMPL) -> String {
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

        format!(
            "requested impl 0x{requested:x}, got impl 0x{implementation:x}, API {}.{}, device 0x{:x}, code name {}",
            version.Major, version.Minor, platform.DeviceId, platform.CodeName,
        )
    }

    fn log_identity(&self, requested: mfxIMPL) {
        log::debug!(
            "[qsv] legacy Media SDK session: {}",
            self.identity(requested)
        );
    }

    fn dump(&self, requested: mfxIMPL, out: &mut String) {
        let _ = writeln!(out, "session: {}", self.identity(requested));
        let _ = writeln!(
            out,
            "cells are raw mfxStatus. the probe accepts only NONE for upload, rotation and\n\
             composite, and any warning but PARTIAL_ACCEL for convert. RGB4 is sent as 4:2:0\n\
             like the probe does; RGB4/444 is the chroma format ffmpeg uses for bgra frames"
        );

        let formats: Vec<(&str, mfxFrameInfo)> = DUMP_FORMATS
            .iter()
            .map(|(name, fourcc, bit_depth, chroma)| {
                let mut info = frame_info(*fourcc, *bit_depth);
                info.ChromaFormat = *chroma;
                (*name, info)
            })
            .collect();
        let columns: Vec<&str> = formats.iter().map(|(name, _)| *name).collect();

        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "convert, video -> video (rows: input, columns: output)"
        );
        dump_header(out, &columns);
        for (name, input) in &formats {
            let cells: Vec<String> = formats
                .iter()
                .map(|(_, output)| status_name(self.vpp_status(*input, *output)))
                .collect();
            dump_row(out, name, &cells);
        }

        let _ = writeln!(out);
        let _ = writeln!(out, "upload, system -> video, same format (hwupload)");
        for (name, info) in &formats {
            dump_row(out, name, &[status_name(self.upload_status(*info))]);
        }

        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "rotation, video -> video, same format (status / angle the runtime echoed)"
        );
        dump_header(out, &["90", "180", "270"]);
        for (name, info) in &formats {
            let cells: Vec<String> = [90u16, 180, 270]
                .into_iter()
                .map(|angle| {
                    let (status, echoed) = self.rotate_status(*info, angle);
                    format!("{} / {echoed}", status_name(status))
                })
                .collect();
            dump_row(out, name, &cells);
        }

        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "composite, 640x480 into 1920x1080 (vpp_qsv pad; rows: input, columns: output)"
        );
        dump_header(out, &columns);
        for (name, input) in &formats {
            let cells: Vec<String> = formats
                .iter()
                .map(|(_, output)| status_name(self.composite_status(*input, *output)))
                .collect();
            dump_row(out, name, &cells);
        }

        for (init, label) in [(false, "Query"), (true, "Init, system memory")] {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "tonemap {label}, VIDEO_SIGNAL_INFO_IN bt2020/PQ -> _OUT bt709 (rows: input, columns: output)"
            );
            dump_header(out, &columns);
            for (name, input) in &formats {
                let cells: Vec<String> = formats
                    .iter()
                    .map(|(_, output)| status_name(self.tonemap_status(*input, *output, init)))
                    .collect();
                dump_row(out, name, &cells);
            }
        }
        let _ = writeln!(out);
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
            if self.can_upload(*fourcc, *bit_depth)
                && (self.can_vpp((*fourcc, *bit_depth), (MFX_FOURCC_NV12, 8))
                    || self.can_vpp((MFX_FOURCC_NV12, 8), (*fourcc, *bit_depth)))
            {
                vpp_pixel_formats.insert(QsvFourCC(*fourcc));
            }
        }

        QsvCapabilities {
            supported_decoders,
            supported_encoders,
            vpp_pixel_formats,
            // 1.x runtime does not provide a filter list
            vpp_filters: HashSet::new(),
            rotation_formats: VPP_FORMATS
                .iter()
                .filter(|(fourcc, bit_depth)| {
                    self.api_version().is_some_and(|version| version >= (1, 17))
                        && [90, 180, 270]
                            .into_iter()
                            .all(|angle| self.can_rotate(*fourcc, *bit_depth, angle))
                })
                .map(|(fourcc, _)| QsvFourCC(*fourcc))
                .collect(),
            composite_formats: COMPOSITE_FORMATS
                .iter()
                .flat_map(|from| COMPOSITE_FORMATS.iter().map(move |to| (*from, *to)))
                .filter(|(from, to)| self.can_composite(*from, *to))
                .map(|(from, to)| (QsvFourCC(from.0), QsvFourCC(to.0)))
                .collect(),
            runtime_api: self.api_version(),
        }
    }

    fn api_version(&self) -> Option<(u16, u16)> {
        let mut version = mfxVersion::default();
        let status = unsafe { (self.vpl.MFXQueryVersion)(self.handle, &mut version) };
        (status == MFX_ERR_NONE).then_some((version.Major, version.Minor))
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
        accepted(self.vpp_status(frame_info(from.0, from.1), frame_info(to.0, to.1)))
    }

    fn vpp_status(&self, from: mfxFrameInfo, to: mfxFrameInfo) -> mfxStatus {
        let vpp = mfxInfoVPP {
            In: from,
            Out: to,
            ..Default::default()
        };

        let mut input = mfxVideoParam::zeroed();
        input.u = mfxVideoParamUnion { vpp };
        input.IOPattern = MFX_IOPATTERN_IN_VIDEO_MEMORY | MFX_IOPATTERN_OUT_VIDEO_MEMORY;

        self.query_status(Query::Vpp, 0, &mut input)
    }

    /// hwupload uses a VPP session to copy frames into video memory. This session
    /// keeps the same format (hwcontext_qsv). The session opens only when the
    /// runtime returns `MFX_ERR_NONE`. Haswell returns `MFX_ERR_NONE` for P010 to
    /// NV12. It returns partial acceleration for P010 to P010. Thus hwupload cannot
    /// copy P010 frames to the GPU on Haswell.
    fn can_upload(&self, fourcc: u32, bit_depth: u8) -> bool {
        self.upload_status(frame_info(fourcc, bit_depth)) == MFX_ERR_NONE
    }

    fn upload_status(&self, info: mfxFrameInfo) -> mfxStatus {
        let vpp = mfxInfoVPP {
            In: info,
            Out: info,
            ..Default::default()
        };

        let mut input = mfxVideoParam::zeroed();
        input.u = mfxVideoParamUnion { vpp };
        input.IOPattern = MFX_IOPATTERN_IN_SYSTEM_MEMORY | MFX_IOPATTERN_OUT_VIDEO_MEMORY;
        let mut output = input;

        let status = unsafe { (self.vpl.MFXVideoVPP_Query)(self.handle, &mut input, &mut output) };
        log::debug!(
            "[qsv] legacy upload query {:?}: {status}",
            QsvFourCC(info.FourCC)
        );

        status
    }

    fn can_rotate(&self, fourcc: u32, bit_depth: u8, angle: u16) -> bool {
        // Warnings can mean the runtime skipped rotation or changed the requested settings.
        let (status, echoed) = self.rotate_status(frame_info(fourcc, bit_depth), angle);
        status == MFX_ERR_NONE && echoed == angle
    }

    /// Returns the status and the angle the runtime wrote back.
    fn rotate_status(&self, source: mfxFrameInfo, angle: u16) -> (mfxStatus, u16) {
        let mut rotation = mfxExtVPPRotation {
            Header: mfxExtBuffer {
                BufferId: MFX_EXTBUFF_VPP_ROTATION,
                BufferSz: std::mem::size_of::<mfxExtVPPRotation>() as u32,
            },
            Angle: angle,
            ..Default::default()
        };
        let mut output_rotation = rotation;
        let mut input_ext = &mut rotation as *mut mfxExtVPPRotation;
        let mut output_ext = &mut output_rotation as *mut mfxExtVPPRotation;
        let mut target = source;
        if angle != 180 {
            std::mem::swap(&mut target.Width, &mut target.Height);
            std::mem::swap(&mut target.CropW, &mut target.CropH);
        }
        let mut input = mfxVideoParam::zeroed();
        input.u = mfxVideoParamUnion {
            vpp: mfxInfoVPP {
                In: source,
                Out: target,
                ..Default::default()
            },
        };
        input.IOPattern = MFX_IOPATTERN_IN_VIDEO_MEMORY | MFX_IOPATTERN_OUT_VIDEO_MEMORY;
        input.ExtParam = (&mut input_ext as *mut *mut mfxExtVPPRotation).cast();
        input.NumExtParam = 1;
        let mut output = input;
        output.ExtParam = (&mut output_ext as *mut *mut mfxExtVPPRotation).cast();
        let status = unsafe { (self.vpl.MFXVideoVPP_Query)(self.handle, &mut input, &mut output) };
        (status, output_rotation.Angle)
    }

    /// The patched vpp_qsv pads with a single-stream composite, so query a sample 4:3
    /// frame in a 16:9 canvas. Composite docs list only NV12 -> NV12 and RGB output,
    /// and legacy runtimes do reject P010 output, so query each format pair.
    fn can_composite(&self, from: (u32, u8), to: (u32, u8)) -> bool {
        // unlike query(), a warning fails: it can mean the runtime ignored the composite
        self.composite_status(frame_info(from.0, from.1), frame_info(to.0, to.1)) == MFX_ERR_NONE
    }

    fn composite_status(&self, from: mfxFrameInfo, to: mfxFrameInfo) -> mfxStatus {
        let bit_depth = to.BitDepthLuma.max(8);
        let mut input_stream = mfxVPPCompInputStream {
            DstX: 240,
            DstY: 0,
            DstW: 1440,
            DstH: 1080,
            ..Default::default()
        };
        let mut output_stream = input_stream;
        let mut composite = mfxExtVPPComposite {
            Header: mfxExtBuffer {
                BufferId: MFX_EXTBUFF_VPP_COMPOSITE,
                BufferSz: std::mem::size_of::<mfxExtVPPComposite>() as u32,
            },
            Y: 16 << (bit_depth - 8),
            U: 128 << (bit_depth - 8),
            V: 128 << (bit_depth - 8),
            NumTiles: 0,
            reserved1: [0; 23],
            NumInputStream: 1,
            InputStream: &mut input_stream,
        };
        let mut output_composite = composite;
        output_composite.InputStream = &mut output_stream;
        let mut input_ext = &mut composite as *mut mfxExtVPPComposite;
        let mut output_ext = &mut output_composite as *mut mfxExtVPPComposite;

        let mut source = from;
        source.Width = 640;
        source.Height = 480;
        source.CropW = 640;
        source.CropH = 480;

        let mut input = mfxVideoParam::zeroed();
        input.u = mfxVideoParamUnion {
            vpp: mfxInfoVPP {
                In: source,
                Out: to,
                ..Default::default()
            },
        };
        input.IOPattern = MFX_IOPATTERN_IN_VIDEO_MEMORY | MFX_IOPATTERN_OUT_VIDEO_MEMORY;
        input.ExtParam = (&mut input_ext as *mut *mut mfxExtVPPComposite).cast();
        input.NumExtParam = 1;
        let mut output = input;
        output.ExtParam = (&mut output_ext as *mut *mut mfxExtVPPComposite).cast();

        let status = unsafe { (self.vpl.MFXVideoVPP_Query)(self.handle, &mut input, &mut output) };
        log::debug!(
            "[qsv] legacy composite query {:?} -> {:?}: {status}",
            QsvFourCC(from.FourCC),
            QsvFourCC(to.FourCC)
        );

        status
    }

    /// The ext buffers vpp_qsv sends for tonemap=1: signal info in (bt2020/PQ) and out
    /// (bt709). Diagnostic only; the probe does not check tonemap on this path yet.
    ///
    /// On vpl-gpu-rt, Query answers NONE even when the driver has no HDR tonemap
    /// filter. Only Init reports that, as FILTER_SKIPPED, so `init` also runs Init
    /// (in system memory, which needs no allocator) and closes it again.
    fn tonemap_status(&self, from: mfxFrameInfo, to: mfxFrameInfo, init: bool) -> mfxStatus {
        let signal_info = |buffer_id, primaries, transfer, matrix| mfxExtVideoSignalInfo {
            Header: mfxExtBuffer {
                BufferId: buffer_id,
                BufferSz: std::mem::size_of::<mfxExtVideoSignalInfo>() as u32,
            },
            // 5 is "unspecified"
            VideoFormat: 5,
            VideoFullRange: 0,
            ColourDescriptionPresent: 1,
            ColourPrimaries: primaries,
            TransferCharacteristics: transfer,
            MatrixCoefficients: matrix,
        };
        let mut input_buffers = [
            signal_info(MFX_EXTBUFF_VIDEO_SIGNAL_INFO_IN, 9, 16, 9),
            signal_info(MFX_EXTBUFF_VIDEO_SIGNAL_INFO_OUT, 1, 1, 1),
        ];
        let mut output_buffers = input_buffers;
        let mut input_ext: [*mut mfxExtVideoSignalInfo; 2] =
            [&mut input_buffers[0], &mut input_buffers[1]];
        let mut output_ext: [*mut mfxExtVideoSignalInfo; 2] =
            [&mut output_buffers[0], &mut output_buffers[1]];

        let mut input = mfxVideoParam::zeroed();
        input.u = mfxVideoParamUnion {
            vpp: mfxInfoVPP {
                In: from,
                Out: to,
                ..Default::default()
            },
        };
        input.ExtParam = input_ext.as_mut_ptr().cast();
        input.NumExtParam = 2;

        if init {
            input.IOPattern = MFX_IOPATTERN_IN_SYSTEM_MEMORY | MFX_IOPATTERN_OUT_SYSTEM_MEMORY;
            let status = unsafe { (self.vpl.MFXVideoVPP_Init)(self.handle, &mut input) };
            if status >= MFX_ERR_NONE {
                unsafe { (self.vpl.MFXVideoVPP_Close)(self.handle) };
            }
            return status;
        }

        input.IOPattern = MFX_IOPATTERN_IN_VIDEO_MEMORY | MFX_IOPATTERN_OUT_VIDEO_MEMORY;
        let mut output = input;
        output.ExtParam = output_ext.as_mut_ptr().cast();

        unsafe { (self.vpl.MFXVideoVPP_Query)(self.handle, &mut input, &mut output) }
    }

    fn query(&self, kind: Query, codec_id: u32, input: &mut mfxVideoParam) -> bool {
        accepted(self.query_status(kind, codec_id, input))
    }

    fn query_status(&self, kind: Query, codec_id: u32, input: &mut mfxVideoParam) -> mfxStatus {
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

        unsafe { query(self.handle, input, &mut output) }
    }
}

/// A warning means the runtime changed the parameters but can still use the
/// hardware. Partial acceleration means it falls back to software, which is not a
/// capability.
fn accepted(status: mfxStatus) -> bool {
    status >= MFX_ERR_NONE && status != MFX_WRN_PARTIAL_ACCELERATION
}

/// (label, fourcc, bit depth, chroma format) for the diagnostic dump
const DUMP_FORMATS: &[(&str, u32, u8, u16)] = &[
    ("NV12", MFX_FOURCC_NV12, 8, MFX_CHROMAFORMAT_YUV420),
    ("P010", MFX_FOURCC_P010, 10, MFX_CHROMAFORMAT_YUV420),
    ("RGB4", MFX_FOURCC_RGB4, 8, MFX_CHROMAFORMAT_YUV420),
    ("RGB4/444", MFX_FOURCC_RGB4, 8, MFX_CHROMAFORMAT_YUV444),
];

const DUMP_CELL: usize = 22;

fn dump_header(out: &mut String, columns: &[&str]) {
    let _ = write!(out, "  {:<10}", "");
    for column in columns {
        let _ = write!(out, "{column:<DUMP_CELL$}");
    }
    let _ = writeln!(out);
}

fn dump_row(out: &mut String, name: &str, cells: &[String]) {
    let _ = write!(out, "  {name:<10}");
    for cell in cells {
        let _ = write!(out, "{cell:<DUMP_CELL$}");
    }
    let _ = writeln!(out);
}

pub(crate) fn status_name(status: mfxStatus) -> String {
    let name = match status {
        MFX_ERR_NONE => "NONE",
        MFX_ERR_UNSUPPORTED => "UNSUPPORTED",
        MFX_ERR_INCOMPATIBLE_VIDEO_PARAM => "ERR_INCOMPAT",
        MFX_ERR_INVALID_VIDEO_PARAM => "INVALID_PARAM",
        MFX_ERR_UNDEFINED_BEHAVIOR => "UNDEFINED",
        MFX_WRN_PARTIAL_ACCELERATION => "PARTIAL_ACCEL",
        MFX_WRN_INCOMPATIBLE_VIDEO_PARAM => "WRN_INCOMPAT",
        MFX_WRN_VALUE_NOT_CHANGED => "VALUE_NOT_CHANGED",
        MFX_WRN_OUT_OF_RANGE => "OUT_OF_RANGE",
        MFX_WRN_FILTER_SKIPPED => "FILTER_SKIPPED",
        _ => "",
    };
    format!("{status} {name}").trim_end().to_string()
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
