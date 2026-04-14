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

    /// Enumerate available implementations and return capability descriptions.
    ///
    /// Pass `MFX_IMPLCAPS_IMPLDESCSTRUCTURE` as `format` to get a pointer to
    /// `mfxImplDescription` back via `desc`. The caller must release it with
    /// `MFXDispReleaseImplDescription` when done.
    pub fn MFXEnumImplementations(
        loader: mfxLoader,
        index: u32,
        format: u32, // mfxImplCapsDeliveryFormat
        desc: *mut mfxHDL,
    ) -> mfxStatus;

    /// Release a capability description handle returned by `MFXEnumImplementations`.
    pub fn MFXDispReleaseImplDescription(loader: mfxLoader, hdl: mfxHDL) -> mfxStatus;

    pub fn MFXUnload(loader: mfxLoader);
}
