#![allow(non_snake_case)]

use std::ffi::{c_char, c_int, c_uint, c_void};

use libloading::Library;

use crate::{VAConfigID, VADisplay, VAEntrypoint, VAProfile, VAStatus, VASurfaceAttrib};

pub struct VaLib {
    _libva: Library,
    _libva_drm: Library,
    pub vaGetDisplayDRM: unsafe extern "C" fn(c_int) -> VADisplay,
    pub vaInitialize: unsafe extern "C" fn(VADisplay, *mut c_int, *mut c_int) -> VAStatus,
    pub vaTerminate: unsafe extern "C" fn(VADisplay) -> VAStatus,
    pub vaQueryVendorString: unsafe extern "C" fn(VADisplay) -> *const c_char,
    pub vaMaxNumProfiles: unsafe extern "C" fn(VADisplay) -> c_int,
    pub vaMaxNumEntrypoints: unsafe extern "C" fn(VADisplay) -> c_int,
    pub vaQueryConfigProfiles:
        unsafe extern "C" fn(VADisplay, *mut VAProfile, *mut c_int) -> VAStatus,
    pub vaQueryConfigEntrypoints:
        unsafe extern "C" fn(VADisplay, VAProfile, *mut VAEntrypoint, *mut c_int) -> VAStatus,
    pub vaCreateConfig: unsafe extern "C" fn(
        VADisplay,
        VAProfile,
        VAEntrypoint,
        *mut c_void,
        c_int,
        *mut VAConfigID,
    ) -> VAStatus,
    pub vaDestroyConfig: unsafe extern "C" fn(VADisplay, VAConfigID) -> VAStatus,
    pub vaQuerySurfaceAttributes:
        unsafe extern "C" fn(VADisplay, VAConfigID, *mut VASurfaceAttrib, *mut c_uint) -> VAStatus,
}

impl VaLib {
    pub fn load() -> Result<Self, libloading::Error> {
        unsafe {
            let lib = Library::new("libva.so.2")?;
            let vaInitialize = *lib.get(b"vaInitialize\0")?;
            let vaTerminate = *lib.get(b"vaTerminate\0")?;
            let vaQueryVendorString = *lib.get(b"vaQueryVendorString\0")?;
            let vaMaxNumProfiles = *lib.get(b"vaMaxNumProfiles\0")?;
            let vaMaxNumEntrypoints = *lib.get(b"vaMaxNumEntrypoints\0")?;
            let vaQueryConfigProfiles = *lib.get(b"vaQueryConfigProfiles\0")?;
            let vaQueryConfigEntrypoints = *lib.get(b"vaQueryConfigEntrypoints\0")?;
            let vaCreateConfig = *lib.get(b"vaCreateConfig\0")?;
            let vaDestroyConfig = *lib.get(b"vaDestroyConfig\0")?;
            let vaQuerySurfaceAttributes = *lib.get(b"vaQuerySurfaceAttributes\0")?;
            let lib_drm = Library::new("libva-drm.so.2")?;
            let vaGetDisplayDRM = *lib_drm.get(b"vaGetDisplayDRM\0")?;
            Ok(Self {
                _libva: lib,
                _libva_drm: lib_drm,
                vaGetDisplayDRM,
                vaInitialize,
                vaTerminate,
                vaQueryVendorString,
                vaMaxNumProfiles,
                vaMaxNumEntrypoints,
                vaQueryConfigProfiles,
                vaQueryConfigEntrypoints,
                vaCreateConfig,
                vaDestroyConfig,
                vaQuerySurfaceAttributes,
            })
        }
    }
}
