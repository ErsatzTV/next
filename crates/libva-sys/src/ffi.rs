#![allow(non_snake_case)]

use std::ffi::{c_char, c_int, c_uint, c_void};

use libloading::Library;

use crate::{
    VAConfigID, VAContextID, VADisplay, VAEntrypoint, VAProcFilterCapHighDynamicRange,
    VAProcFilterType, VAProcPipelineCaps, VAProfile, VAStatus, VASurfaceAttrib, VASurfaceID,
};

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
    pub vaCreateContext: unsafe extern "C" fn(
        VADisplay,
        VAConfigID,
        picture_width: c_int,
        picture_height: c_int,
        flag: c_int,
        render_targets: *mut VASurfaceID,
        num_render_targets: c_int,
        context: *mut VAContextID,
    ) -> VAStatus,
    pub vaDestroyContext: unsafe extern "C" fn(VADisplay, VAContextID) -> VAStatus,
    pub vaQueryVideoProcFilterCaps: unsafe extern "C" fn(
        dpy: VADisplay,
        ctx_id: VAContextID,
        filter_type: VAProcFilterType,
        filter_caps: *mut VAProcFilterCapHighDynamicRange,
        num_filter_caps: *mut c_uint,
    ) -> VAStatus,
    pub vaQueryVideoProcPipelineCaps: unsafe extern "C" fn(
        dpy: VADisplay,
        ctx_id: VAContextID,
        filters: *mut c_uint,
        num_filters: c_uint,
        pipeline_caps: *mut VAProcPipelineCaps,
    ) -> VAStatus,
    pub vaCreateSurfaces: unsafe extern "C" fn(
        dpy: VADisplay,
        format: c_uint,
        width: c_uint,
        height: c_uint,
        surfaces: *mut VASurfaceID,
        num_surfaces: c_uint,
        attrib_list: *mut VASurfaceAttrib,
        num_attibs: c_uint,
    ) -> VAStatus,
    pub vaDestroySurfaces: unsafe extern "C" fn(
        dpy: VADisplay,
        surfaces: *mut VASurfaceID,
        num_surfaces: c_uint,
    ) -> VAStatus,
    pub vaErrorStr: unsafe extern "C" fn(VAStatus) -> *const c_char,
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
            let vaCreateContext = *lib.get(b"vaCreateContext\0")?;
            let vaDestroyContext = *lib.get(b"vaDestroyContext\0")?;
            let vaQueryVideoProcFilterCaps = *lib.get(b"vaQueryVideoProcFilterCaps\0")?;
            let vaQueryVideoProcPipelineCaps = *lib.get(b"vaQueryVideoProcPipelineCaps\0")?;
            let vaCreateSurfaces = *lib.get(b"vaCreateSurfaces\0")?;
            let vaDestroySurfaces = *lib.get(b"vaDestroySurfaces\0")?;
            let vaErrorStr = *lib.get(b"vaErrorStr\0")?;
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
                vaCreateContext,
                vaDestroyContext,
                vaQueryVideoProcFilterCaps,
                vaQueryVideoProcPipelineCaps,
                vaCreateSurfaces,
                vaDestroySurfaces,
                vaErrorStr,
            })
        }
    }
}
