#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::c_void;

pub type mfxStatus = i32;
pub type mfxLoader = *mut c_void;
pub type mfxConfig = *mut c_void;
pub type mfxHDL = *mut c_void;

pub const MFX_ERR_NONE: mfxStatus = 0;

/// mfxVariant.Type value for u32 payload
pub const MFX_VARIANT_TYPE_U32: u32 = 5;

/// mfxImplType: hardware implementation (used with MFXSetConfigFilterProperty)
pub const MFX_IMPL_TYPE_HARDWARE: u32 = 2;

/// mfxImplCapsDeliveryFormat: return mfxImplDescription struct
pub const MFX_IMPLCAPS_IMPLDESCSTRUCTURE: u32 = 1;

pub const MFX_CODEC_AVC: u32 = u32::from_ne_bytes(*b"AVC ");
pub const MFX_CODEC_HEVC: u32 = u32::from_ne_bytes(*b"HEVC");
pub const MFX_CODEC_MPEG2: u32 = u32::from_ne_bytes(*b"MPG2");
pub const MFX_CODEC_VC1: u32 = u32::from_ne_bytes(*b"VC1 ");
pub const MFX_CODEC_VP8: u32 = u32::from_ne_bytes(*b"VP8 ");
pub const MFX_CODEC_VP9: u32 = u32::from_ne_bytes(*b"VP9 ");
pub const MFX_CODEC_AV1: u32 = u32::from_ne_bytes(*b"AV1 ");

// H.264 profiles (subset used for bit-depth detection)
pub const MFX_PROFILE_AVC_HIGH10: u32 = 110;

// HEVC profiles (subset used for bit-depth detection)
pub const MFX_PROFILE_HEVC_MAIN10: u32 = 2;

// VP9 10-bit profiles (Profile 2 = 10/12-bit 4:2:0, Profile 3 = 10/12-bit 4:4:4).
// the mfx enum is 1-based, so each value is the VP9 profile number plus one.
pub const MFX_PROFILE_VP9_2: u32 = 3;
pub const MFX_PROFILE_VP9_3: u32 = 4;

// AV1: Main profile supports 8 and 10-bit, so treat any profile as potentially 10-bit capable

pub const MFX_FOURCC_NV12: u32 = u32::from_ne_bytes(*b"NV12");
pub const MFX_FOURCC_P010: u32 = u32::from_ne_bytes(*b"P010");
pub const MFX_FOURCC_RGB4: u32 = u32::from_ne_bytes(*b"RGB4");

#[repr(C)]
#[derive(Copy, Clone)]
pub union mfxVariantValue {
    pub U8: u8,
    pub U16: u16,
    pub U32: u32,
    pub U64: u64,
    pub I16: i16,
    pub I32: i32,
    pub I64: i64,
    pub F32: f32,
    pub F64: f64,
    pub PTR: *mut c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxVariant {
    pub Version: u32,
    pub Type: u32,
    pub Data: mfxVariantValue,
}

/// Opaque version word shared by all mfx*Description structs (2 bytes).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxStructVersion {
    pub Version: u16,
}

/// Top-level decoder capability list returned inside mfxImplDescription.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxDecoderDescription {
    pub Version: mfxStructVersion,
    pub reserved: [u16; 7],
    pub NumCodecs: u16,
    // 6 bytes implicit padding (repr(C) aligns *mut to 8)
    pub Codecs: *mut mfxDecoderDescription_decoder,
}

/// Per-codec entry inside mfxDecoderDescription (size 32).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxDecoderDescription_decoder {
    pub CodecID: u32,
    pub reserved: [u16; 8],
    pub MaxcodecLevel: u16,
    pub NumProfiles: u16,
    pub Profiles: *mut mfxDecoderDescription_decoder_decprofile,
}

/// Per-profile entry for a decoder codec (size 32).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxDecoderDescription_decoder_decprofile {
    pub Profile: u32,
    pub reserved: [u16; 7],
    pub NumMemTypes: u16,
    // 4 bytes implicit padding
    pub MemDesc: *mut c_void,
}

/// Top-level encoder capability list returned inside mfxImplDescription.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxEncoderDescription {
    pub Version: mfxStructVersion,
    pub reserved: [u16; 7],
    pub NumCodecs: u16,
    // 6 bytes implicit padding
    pub Codecs: *mut mfxEncoderDescription_encoder,
}

/// Per-codec entry inside mfxEncoderDescription (size 32).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxEncoderDescription_encoder {
    pub CodecID: u32,
    pub MaxcodecLevel: u16,
    pub BiDirectionalPrediction: u16,
    pub reserved: [u16; 7],
    pub NumProfiles: u16,
    pub Profiles: *mut mfxEncoderDescription_encoder_encprofile,
}

/// Per-profile entry for an encoder codec (size 32).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxEncoderDescription_encoder_encprofile {
    pub Profile: u32,
    pub reserved: [u16; 7],
    pub NumMemTypes: u16,
    // 4 bytes implicit padding
    pub MemDesc: *mut c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxVPPDescription {
    pub Version: mfxStructVersion,
    pub reserved: [u16; 7],
    pub NumFilters: u16,
    // 6 bytes implicit padding
    pub Filters: *mut mfxVPPDescription_filter,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxVPPDescription_filter {
    pub FilterFourCC: u32,
    pub MaxDelayInFrames: u16,
    pub reserved: [u16; 7],
    pub NumMemTypes: u16,
    // 4 bytes implicit padding
    pub MemDesc: *mut mfxVPPDescription_filter_memdesc,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxVPPDescription_filter_memdesc {
    pub MemHandleType: u32,
    pub Width: mfxRange32U,
    pub Height: mfxRange32U,
    pub reserved: [u16; 7],
    pub NumInFormats: u16,
    // 4 bytes implicit padding
    pub Formats: *mut mfxVPPDescription_filter_memdesc_format,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxVPPDescription_filter_memdesc_format {
    pub InFormat: u32,
    pub reserved: [u16; 5],
    pub NumOutFormat: u16,
    // 4 bytes implicit padding on 64-bit
    pub OutFormats: *mut u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxRange32U {
    pub Min: u32,
    pub Max: u32,
    pub Step: u32,
}

// legacy Intel Media SDK types (API 1.x). the dispatcher fills the
// mfxImplDescription tree only for API 2.x runtimes, so a legacy runtime must be
// asked about one codec at a time with the 1.x Query functions.

pub type mfxSession = *mut c_void;
pub type mfxIMPL = i32;

pub const MFX_ERR_UNSUPPORTED: mfxStatus = -3;
pub const MFX_WRN_PARTIAL_ACCELERATION: mfxStatus = 4;

pub const MFX_IMPL_HARDWARE_ANY: mfxIMPL = 0x0004;
pub const MFX_IMPL_VIA_D3D9: mfxIMPL = 0x0200;
pub const MFX_IMPL_VIA_D3D11: mfxIMPL = 0x0300;
pub const MFX_IMPL_VIA_VAAPI: mfxIMPL = 0x0400;

pub const MFX_HANDLE_D3D9_DEVICE_MANAGER: u32 = 1;
pub const MFX_HANDLE_D3D11_DEVICE: u32 = 3;
pub const MFX_HANDLE_VA_DISPLAY: u32 = 4;

pub const MFX_IOPATTERN_IN_VIDEO_MEMORY: u16 = 0x01;
pub const MFX_IOPATTERN_OUT_VIDEO_MEMORY: u16 = 0x10;

pub const MFX_PICSTRUCT_PROGRESSIVE: u16 = 0x01;
pub const MFX_CHROMAFORMAT_YUV420: u16 = 1;
pub const MFX_RATECONTROL_CQP: u16 = 3;
pub const MFX_TARGETUSAGE_BALANCED: u16 = 4;

/// mfxVersion, laid out as the struct arm of the union in the header.
#[repr(C)]
#[derive(Copy, Clone, Default, Debug)]
pub struct mfxVersion {
    pub Minor: u16,
    pub Major: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Default, Debug)]
pub struct mfxPlatform {
    pub CodeName: u16,
    pub DeviceId: u16,
    pub MediaAdapterType: u16,
    pub reserved: [u16; 13],
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct mfxFrameId {
    pub TemporalId: u16,
    pub PriorityId: u16,
    pub DependencyId: u16,
    pub QualityId: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct mfxFrameInfo {
    pub reserved: [u32; 4],
    pub ChannelId: u16,
    pub BitDepthLuma: u16,
    pub BitDepthChroma: u16,
    pub Shift: u16,
    pub FrameId: mfxFrameId,
    pub FourCC: u32,
    pub Width: u16,
    pub Height: u16,
    pub CropX: u16,
    pub CropY: u16,
    pub CropW: u16,
    pub CropH: u16,
    pub FrameRateExtN: u32,
    pub FrameRateExtD: u32,
    pub reserved3: u16,
    pub AspectRatioW: u16,
    pub AspectRatioH: u16,
    pub PicStruct: u16,
    pub ChromaFormat: u16,
    pub reserved2: u16,
}

/// The fields from `TargetUsage` on are the encode arm of an anonymous union. it is
/// the largest arm, so it also sets the size of the struct.
#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct mfxInfoMFX {
    pub reserved: [u32; 7],
    pub LowPower: u16,
    pub BRCParamMultiplier: u16,
    pub FrameInfo: mfxFrameInfo,
    pub CodecId: u32,
    pub CodecProfile: u16,
    pub CodecLevel: u16,
    pub NumThread: u16,
    pub TargetUsage: u16,
    pub GopPicSize: u16,
    pub GopRefDist: u16,
    pub GopOptFlag: u16,
    pub IdrInterval: u16,
    pub RateControlMethod: u16,
    pub QPI: u16,
    pub BufferSizeInKB: u16,
    pub QPP: u16,
    pub QPB: u16,
    pub NumSlice: u16,
    pub NumRefFrame: u16,
    pub EncodedOrder: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct mfxInfoVPP {
    pub reserved: [u32; 8],
    pub In: mfxFrameInfo,
    pub Out: mfxFrameInfo,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union mfxVideoParamUnion {
    pub mfx: mfxInfoMFX,
    pub vpp: mfxInfoVPP,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxVideoParam {
    pub AllocId: u32,
    pub reserved: [u32; 2],
    pub reserved3: u16,
    pub AsyncDepth: u16,
    pub u: mfxVideoParamUnion,
    pub Protected: u16,
    pub IOPattern: u16,
    pub ExtParam: *mut c_void,
    pub NumExtParam: u16,
    pub reserved2: [u16; 3],
}

impl mfxVideoParam {
    pub fn zeroed() -> Self {
        // every field is an integer, a pointer, or an array of those
        unsafe { std::mem::zeroed() }
    }
}

#[cfg(test)]
mod layout_tests {
    use std::mem::{align_of, offset_of, size_of};

    use super::*;

    // values taken from the oneVPL headers with offsetof and sizeof on x86_64
    #[test]
    fn mfx_frame_info_matches_header() {
        assert_eq!(size_of::<mfxFrameInfo>(), 68);
        assert_eq!(align_of::<mfxFrameInfo>(), 4);
        assert_eq!(size_of::<mfxFrameId>(), 8);
        assert_eq!(offset_of!(mfxFrameInfo, ChannelId), 16);
        assert_eq!(offset_of!(mfxFrameInfo, BitDepthLuma), 18);
        assert_eq!(offset_of!(mfxFrameInfo, BitDepthChroma), 20);
        assert_eq!(offset_of!(mfxFrameInfo, Shift), 22);
        assert_eq!(offset_of!(mfxFrameInfo, FrameId), 24);
        assert_eq!(offset_of!(mfxFrameInfo, FourCC), 32);
        assert_eq!(offset_of!(mfxFrameInfo, Width), 36);
        assert_eq!(offset_of!(mfxFrameInfo, Height), 38);
        assert_eq!(offset_of!(mfxFrameInfo, CropX), 40);
        assert_eq!(offset_of!(mfxFrameInfo, CropW), 44);
        assert_eq!(offset_of!(mfxFrameInfo, CropH), 46);
        assert_eq!(offset_of!(mfxFrameInfo, FrameRateExtN), 48);
        assert_eq!(offset_of!(mfxFrameInfo, FrameRateExtD), 52);
        assert_eq!(offset_of!(mfxFrameInfo, AspectRatioW), 58);
        assert_eq!(offset_of!(mfxFrameInfo, PicStruct), 62);
        assert_eq!(offset_of!(mfxFrameInfo, ChromaFormat), 64);
    }

    #[test]
    fn mfx_info_mfx_matches_header() {
        assert_eq!(size_of::<mfxInfoMFX>(), 136);
        assert_eq!(align_of::<mfxInfoMFX>(), 4);
        assert_eq!(offset_of!(mfxInfoMFX, LowPower), 28);
        assert_eq!(offset_of!(mfxInfoMFX, BRCParamMultiplier), 30);
        assert_eq!(offset_of!(mfxInfoMFX, FrameInfo), 32);
        assert_eq!(offset_of!(mfxInfoMFX, CodecId), 100);
        assert_eq!(offset_of!(mfxInfoMFX, CodecProfile), 104);
        assert_eq!(offset_of!(mfxInfoMFX, CodecLevel), 106);
        assert_eq!(offset_of!(mfxInfoMFX, NumThread), 108);
        assert_eq!(offset_of!(mfxInfoMFX, TargetUsage), 110);
        assert_eq!(offset_of!(mfxInfoMFX, GopPicSize), 112);
        assert_eq!(offset_of!(mfxInfoMFX, GopRefDist), 114);
        assert_eq!(offset_of!(mfxInfoMFX, RateControlMethod), 120);
        assert_eq!(offset_of!(mfxInfoMFX, QPI), 122);
        assert_eq!(offset_of!(mfxInfoMFX, QPP), 126);
        assert_eq!(offset_of!(mfxInfoMFX, QPB), 128);
    }

    #[test]
    fn mfx_info_vpp_matches_header() {
        assert_eq!(size_of::<mfxInfoVPP>(), 168);
        assert_eq!(offset_of!(mfxInfoVPP, In), 32);
        assert_eq!(offset_of!(mfxInfoVPP, Out), 100);
    }

    #[test]
    fn mfx_video_param_matches_header() {
        assert_eq!(size_of::<mfxVideoParam>(), 208);
        assert_eq!(align_of::<mfxVideoParam>(), 8);
        assert_eq!(size_of::<mfxVideoParamUnion>(), 168);
        assert_eq!(offset_of!(mfxVideoParam, AsyncDepth), 14);
        assert_eq!(offset_of!(mfxVideoParam, u), 16);
        assert_eq!(offset_of!(mfxVideoParam, Protected), 184);
        assert_eq!(offset_of!(mfxVideoParam, IOPattern), 186);
        assert_eq!(offset_of!(mfxVideoParam, ExtParam), 192);
        assert_eq!(offset_of!(mfxVideoParam, NumExtParam), 200);
    }

    #[test]
    fn mfx_platform_matches_header() {
        assert_eq!(size_of::<mfxPlatform>(), 32);
        assert_eq!(size_of::<mfxVersion>(), 4);
    }
}

#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
mod ffi;

#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
pub use ffi::*;
