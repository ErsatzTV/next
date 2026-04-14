#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::c_void;

pub type mfxStatus = i32;
pub type mfxLoader = *mut c_void;
pub type mfxConfig = *mut c_void;
pub type mfxSession = *mut c_void;

pub const MFX_ERR_NONE: mfxStatus = 0;
pub const MFX_ERR_NOT_FOUND: mfxStatus = -9;

pub const MFX_VARIANT_VALUE_TYPE_U32: u32 = 4;

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

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxFrameInfo {
    pub reserved: [u32; 4],
    pub FourCC: u32,
    pub Width: u16,
    pub Height: u16,
    pub CropX: u16,
    pub CropY: u16,
    pub CropW: u16,
    pub CropH: u16,
    pub FrameRateExtN: u32,
    pub FrameRateExtD: u32,
    pub reserved2: u16,
    pub AspectRatioW: u16,
    pub AspectRatioH: u16,
    pub PicStruct: u16,
    pub ChromaFormat: u16,
    pub reserved3: u16,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxVideoParam {
    pub reserved: [u32; 2],
    pub mfx: mfxInfoMFX,
    pub AsyncDepth: u16,
    pub ExtParam: *mut *mut c_void,
    pub NumExtParam: u16,
    pub reserved2: u16,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mfxInfoMFX {
    pub reserved: [u32; 7],
    pub LowPower: u16,
    pub BRCType: u16,
    pub FrameInfo: mfxFrameInfo,
    pub CodecId: u32,
    pub CodecProfile: u16,
    pub CodecLevel: u16,
    pub NumThread: u16,
    pub reserved2: [u16; 3],
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod ffi;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use ffi::*;
