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
            Ok(Self {
                _lib: lib,
                MFXLoad,
                MFXCreateConfig,
                MFXSetConfigFilterProperty,
                MFXEnumImplementations,
                MFXDispReleaseImplDescription,
                MFXUnload,
            })
        }
    }
}
