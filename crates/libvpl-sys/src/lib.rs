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
    pub U32: u32,
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

unsafe extern "C" {
    pub fn MFXLoad() -> mfxLoader;

    pub fn MFXCreateConfig(loader: mfxLoader) -> mfxConfig;
    pub fn MFXSetConfigFilterProperty(
        config: mfxConfig,
        name: *const u8, // null-terminated string
        value: mfxVariantValue,
    ) -> mfxStatus;

    pub fn MFXCreateSession(loader: mfxLoader, index: u32, session: *mut mfxSession) -> mfxStatus;

    pub fn MFXUnload(loader: mfxLoader);
    pub fn MFXClose(session: mfxSession) -> mfxStatus;

    pub fn MFXVideoDECODE_Query(
        session: mfxSession,
        in_param: *mut mfxVideoParam,
        out_param: *mut mfxVideoParam,
    ) -> mfxStatus;

    pub fn MFXVideoENCODE_Query(
        session: mfxSession,
        in_param: *mut mfxVideoParam,
        out_param: *mut mfxVideoParam,
    ) -> mfxStatus;
}
