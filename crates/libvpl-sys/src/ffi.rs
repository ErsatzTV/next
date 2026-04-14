#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::*;

unsafe extern "C" {
    pub fn MFXLoad() -> mfxLoader;

    pub fn MFXCreateConfig(loader: mfxLoader) -> mfxConfig;
    pub fn MFXSetConfigFilterProperty(
        config: mfxConfig,
        name: *const u8, // null-terminated string
        value: mfxVariant,
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
