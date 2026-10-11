#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use libloading::Library;

use crate::*;

pub struct VplLib {
    _lib: Library,
    pub MFXLoad: unsafe extern "C" fn() -> mfxLoader,
    pub MFXCreateConfig: unsafe extern "C" fn(mfxLoader) -> mfxConfig,
    pub MFXSetConfigFilterProperty:
        unsafe extern "C" fn(mfxConfig, *const u8, mfxVariant) -> mfxStatus,
    pub MFXEnumImplementations: unsafe extern "C" fn(mfxLoader, u32, u32, *mut mfxHDL) -> mfxStatus,
    pub MFXDispReleaseImplDescription: unsafe extern "C" fn(mfxLoader, mfxHDL) -> mfxStatus,
    pub MFXUnload: unsafe extern "C" fn(mfxLoader),
    // API 1.x entry points, for runtimes the dispatcher cannot describe
    pub MFXInit: unsafe extern "C" fn(mfxIMPL, *mut mfxVersion, *mut mfxSession) -> mfxStatus,
    pub MFXClose: unsafe extern "C" fn(mfxSession) -> mfxStatus,
    pub MFXQueryVersion: unsafe extern "C" fn(mfxSession, *mut mfxVersion) -> mfxStatus,
    pub MFXQueryIMPL: unsafe extern "C" fn(mfxSession, *mut mfxIMPL) -> mfxStatus,
    pub MFXVideoCORE_SetHandle: unsafe extern "C" fn(mfxSession, u32, mfxHDL) -> mfxStatus,
    pub MFXVideoCORE_QueryPlatform: unsafe extern "C" fn(mfxSession, *mut mfxPlatform) -> mfxStatus,
    pub MFXVideoENCODE_Query:
        unsafe extern "C" fn(mfxSession, *mut mfxVideoParam, *mut mfxVideoParam) -> mfxStatus,
    pub MFXVideoDECODE_Query:
        unsafe extern "C" fn(mfxSession, *mut mfxVideoParam, *mut mfxVideoParam) -> mfxStatus,
    pub MFXVideoVPP_Query:
        unsafe extern "C" fn(mfxSession, *mut mfxVideoParam, *mut mfxVideoParam) -> mfxStatus,
}

impl VplLib {
    pub fn load() -> Result<Self, libloading::Error> {
        #[cfg(target_os = "linux")]
        let name = "libvpl.so.2";
        #[cfg(target_os = "windows")]
        let name = "libvpl.dll";
        unsafe {
            let lib = Library::new(name)?;
            let MFXLoad = *lib.get(b"MFXLoad\0")?;
            let MFXCreateConfig = *lib.get(b"MFXCreateConfig\0")?;
            let MFXSetConfigFilterProperty = *lib.get(b"MFXSetConfigFilterProperty\0")?;
            let MFXEnumImplementations = *lib.get(b"MFXEnumImplementations\0")?;
            let MFXDispReleaseImplDescription = *lib.get(b"MFXDispReleaseImplDescription\0")?;
            let MFXUnload = *lib.get(b"MFXUnload\0")?;
            let MFXInit = *lib.get(b"MFXInit\0")?;
            let MFXClose = *lib.get(b"MFXClose\0")?;
            let MFXQueryVersion = *lib.get(b"MFXQueryVersion\0")?;
            let MFXQueryIMPL = *lib.get(b"MFXQueryIMPL\0")?;
            let MFXVideoCORE_SetHandle = *lib.get(b"MFXVideoCORE_SetHandle\0")?;
            let MFXVideoCORE_QueryPlatform = *lib.get(b"MFXVideoCORE_QueryPlatform\0")?;
            let MFXVideoENCODE_Query = *lib.get(b"MFXVideoENCODE_Query\0")?;
            let MFXVideoDECODE_Query = *lib.get(b"MFXVideoDECODE_Query\0")?;
            let MFXVideoVPP_Query = *lib.get(b"MFXVideoVPP_Query\0")?;
            Ok(Self {
                _lib: lib,
                MFXLoad,
                MFXCreateConfig,
                MFXSetConfigFilterProperty,
                MFXEnumImplementations,
                MFXDispReleaseImplDescription,
                MFXUnload,
                MFXInit,
                MFXClose,
                MFXQueryVersion,
                MFXQueryIMPL,
                MFXVideoCORE_SetHandle,
                MFXVideoCORE_QueryPlatform,
                MFXVideoENCODE_Query,
                MFXVideoDECODE_Query,
                MFXVideoVPP_Query,
            })
        }
    }
}
