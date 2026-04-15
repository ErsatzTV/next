use std::collections::HashSet;
use std::ffi::{CStr, c_uint};
use std::fs::File;
use std::os::unix::io::AsRawFd;

use libva_sys::*;

use crate::capabilities::vaapi::VaapiCapabilities;
use crate::error::FFPipelineError;
use crate::error::FFPipelineError::VaapiCapabilitiesError;

impl VaapiCapabilities {
    pub fn probe(device: &str, driver: &str) -> Result<VaapiCapabilities, FFPipelineError> {
        let prev = std::env::var("LIBVA_DRIVER_NAME").ok();
        unsafe { std::env::set_var("LIBVA_DRIVER_NAME", driver) };

        let result = Self::probe_inner(device);

        unsafe {
            match prev {
                Some(v) => std::env::set_var("LIBVA_DRIVER_NAME", v),
                None => std::env::remove_var("LIBVA_DRIVER_NAME"),
            }
        };

        result
    }

    fn probe_inner(device: &str) -> Result<VaapiCapabilities, FFPipelineError> {
        let va =
            VaLib::load().map_err(|e| VaapiCapabilitiesError(format!("libva not found: {e}")))?;

        let file = File::options()
            .read(true)
            .write(true)
            .open(device)
            .map_err(|e| VaapiCapabilitiesError(format!("cannot open {device}: {e}")))?;

        let display = unsafe { (va.vaGetDisplayDRM)(file.as_raw_fd()) };
        if display.is_null() {
            return Err(VaapiCapabilitiesError(String::from(
                "vaGetDisplayDRM returned NULL",
            )));
        }

        let mut major = 0i32;
        let mut minor = 0i32;
        let status = unsafe { (va.vaInitialize)(display, &mut major, &mut minor) };
        if status != VA_STATUS_SUCCESS {
            return Err(VaapiCapabilitiesError(format!(
                "vaInitialize failed: {status}"
            )));
        }

        let vendor = unsafe {
            let ptr = (va.vaQueryVendorString)(display);
            if ptr.is_null() {
                String::new()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        };

        let max_profiles = unsafe { (va.vaMaxNumProfiles)(display) } as usize;
        let mut profiles = vec![0i32; max_profiles];
        let mut num_profiles = 0i32;
        let status = unsafe {
            (va.vaQueryConfigProfiles)(display, profiles.as_mut_ptr(), &mut num_profiles)
        };
        if status != VA_STATUS_SUCCESS {
            unsafe {
                (va.vaTerminate)(display);
            }
            return Err(VaapiCapabilitiesError(format!(
                "vaQueryConfigProfiles failed: {status}"
            )));
        }
        profiles.truncate(num_profiles as usize);

        let max_entrypoints = unsafe { (va.vaMaxNumEntrypoints)(display) } as usize;
        let mut supported = HashSet::new();

        for &profile in &profiles {
            let mut entrypoints = vec![0i32; max_entrypoints];
            let mut num_entrypoints = 0i32;
            let status = unsafe {
                (va.vaQueryConfigEntrypoints)(
                    display,
                    profile,
                    entrypoints.as_mut_ptr(),
                    &mut num_entrypoints,
                )
            };
            if status == VA_STATUS_SUCCESS {
                for &ep in &entrypoints[..num_entrypoints as usize] {
                    supported.insert((profile, ep));
                }
            }
        }

        let mut vpp_pixel_formats = HashSet::new();

        if supported.contains(&(VA_PROFILE_NONE, VA_ENTRYPOINT_VIDEO_PROC)) {
            let mut config_id: VAConfigID = 0;
            let status = unsafe {
                (va.vaCreateConfig)(
                    display,
                    VA_PROFILE_NONE,
                    VA_ENTRYPOINT_VIDEO_PROC,
                    std::ptr::null_mut(),
                    0,
                    &mut config_id,
                )
            };

            if status == VA_STATUS_SUCCESS {
                let mut num_attrs: c_uint = 0;
                let status = unsafe {
                    (va.vaQuerySurfaceAttributes)(
                        display,
                        config_id,
                        std::ptr::null_mut(),
                        &mut num_attrs,
                    )
                };

                if status == VA_STATUS_SUCCESS && num_attrs > 0 {
                    let mut attrs =
                        unsafe { vec![std::mem::zeroed::<VASurfaceAttrib>(); num_attrs as usize] };
                    let status = unsafe {
                        (va.vaQuerySurfaceAttributes)(
                            display,
                            config_id,
                            attrs.as_mut_ptr(),
                            &mut num_attrs,
                        )
                    };

                    if status == VA_STATUS_SUCCESS {
                        for attr in &attrs[..num_attrs as usize] {
                            if attr.type_ == VA_SURFACE_ATTRIB_PIXEL_FORMAT
                                && (attr.flags & VA_SURFACE_ATTRIB_GETTABLE as u32) != 0
                                && attr.value.value_type == VA_GENERIC_VALUE_TYPE_INTEGER
                            {
                                vpp_pixel_formats.insert(unsafe { attr.value.value.i } as u32);
                            }
                        }
                    }
                }

                unsafe { (va.vaDestroyConfig)(display, config_id) };
            }
        }

        unsafe {
            (va.vaTerminate)(display);
        }

        Ok(VaapiCapabilities {
            vendor,
            supported,
            vpp_pixel_formats,
        })
    }
}
