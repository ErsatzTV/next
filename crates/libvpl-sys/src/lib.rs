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

// VP9 10-bit profiles (Profile 2 = 10/12-bit 4:2:0, Profile 3 = 10/12-bit 4:4:4)
pub const MFX_PROFILE_VP9_2: u32 = 2;
pub const MFX_PROFILE_VP9_3: u32 = 3;

// AV1: Main profile supports 8 and 10-bit, so treat any profile as potentially 10-bit capable

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
