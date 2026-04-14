use std::ffi::{c_char, c_int, c_uint, c_void};

use crate::{VAConfigID, VADisplay, VAEntrypoint, VAProfile, VAStatus, VASurfaceAttrib};

unsafe extern "C" {
    pub fn vaGetDisplayDRM(fd: c_int) -> VADisplay;

    pub fn vaInitialize(
        dpy: VADisplay,
        major_version: *mut c_int,
        minor_version: *mut c_int,
    ) -> VAStatus;

    pub fn vaTerminate(dpy: VADisplay) -> VAStatus;

    pub fn vaQueryVendorString(dpy: VADisplay) -> *const c_char;

    pub fn vaMaxNumProfiles(dpy: VADisplay) -> c_int;

    pub fn vaMaxNumEntrypoints(dpy: VADisplay) -> c_int;

    pub fn vaQueryConfigProfiles(
        dpy: VADisplay,
        profile_list: *mut VAProfile,
        num_profiles: *mut c_int,
    ) -> VAStatus;

    pub fn vaQueryConfigEntrypoints(
        dpy: VADisplay,
        profile: VAProfile,
        entrypoint_list: *mut VAEntrypoint,
        num_entrypoints: *mut c_int,
    ) -> VAStatus;

    pub fn vaCreateConfig(
        dpy: VADisplay,
        profile: VAProfile,
        entrypoint: VAEntrypoint,
        attrib_list: *mut c_void,
        num_attribs: c_int,
        config_id: *mut VAConfigID,
    ) -> VAStatus;

    pub fn vaDestroyConfig(dpy: VADisplay, config_id: VAConfigID) -> VAStatus;

    pub fn vaQuerySurfaceAttributes(
        dpy: VADisplay,
        config: VAConfigID,
        attrib_list: *mut VASurfaceAttrib,
        num_attribs: *mut c_uint,
    ) -> VAStatus;
}
