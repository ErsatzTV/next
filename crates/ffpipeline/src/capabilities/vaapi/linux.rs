use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, c_uint};
use std::fs::File;
use std::os::unix::io::AsRawFd;

use libva_sys::*;

use crate::capabilities::vaapi::VaapiCapabilities;
use crate::error::FFPipelineError;
use crate::error::FFPipelineError::VaapiCapabilitiesError;

impl VaapiCapabilities {
    pub fn probe(device: &str, driver: Option<&str>) -> Result<VaapiCapabilities, FFPipelineError> {
        let prev = std::env::var("LIBVA_DRIVER_NAME").ok();
        let prev_level = std::env::var("LIBVA_MESSAGING_LEVEL").ok();
        unsafe {
            if let Some(driver) = driver {
                std::env::set_var("LIBVA_DRIVER_NAME", driver);
            }
            std::env::set_var("LIBVA_MESSAGING_LEVEL", "1");
        };

        let result = Self::probe_inner(device);

        unsafe {
            match prev {
                Some(v) => std::env::set_var("LIBVA_DRIVER_NAME", v),
                None => std::env::remove_var("LIBVA_DRIVER_NAME"),
            }

            match prev_level {
                Some(v) => std::env::set_var("LIBVA_MESSAGING_LEVEL", v),
                None => std::env::remove_var("LIBVA_MESSAGING_LEVEL"),
            }
        };

        result
    }

    fn probe_inner(device: &str) -> Result<VaapiCapabilities, FFPipelineError> {
        log::debug!("[vaapi] probing: device={}", device);
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
                "vaQueryConfigProfiles failed: {}",
                Self::va_status_to_string(&va, status)
            )));
        }
        profiles.truncate(num_profiles as usize);

        let max_entrypoints = unsafe { (va.vaMaxNumEntrypoints)(display) } as usize;
        let mut supported = HashSet::new();
        let mut rate_control = HashMap::new();

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

                    let mut attr = VAConfigAttrib {
                        type_: VA_CONFIG_ATTRIB_RATE_CONTROL,
                        value: 0,
                    };
                    let status =
                        unsafe { (va.vaGetConfigAttributes)(display, profile, ep, &mut attr, 1) };
                    if status == VA_STATUS_SUCCESS && attr.value != VA_ATTRIB_NOT_SUPPORTED {
                        rate_control.insert((profile, ep), attr.value);
                    }
                }
            }
        }

        let mut vpp_pixel_formats = HashSet::new();
        let mut can_hdr_to_hdr_tonemap: HashSet<u32> = HashSet::new();
        let mut can_hdr_to_sdr_tonemap: HashSet<u32> = HashSet::new();
        let mut can_overlay: Option<bool> = None;

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

            // createConfig success block.
            if status == VA_STATUS_SUCCESS {
                // query surface attributes.
                let mut num_attrs: c_uint = 0;
                let status = unsafe {
                    (va.vaQuerySurfaceAttributes)(
                        display,
                        config_id,
                        std::ptr::null_mut(),
                        &mut num_attrs,
                    )
                };

                if status != VA_STATUS_SUCCESS {
                    log::warn!(
                        "vaQuerySurfaceAttributes failed: {}",
                        Self::va_status_to_string(&va, status)
                    );
                } else if status == VA_STATUS_SUCCESS && num_attrs > 0 {
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

                // Query well-known formats for tonemapping support.
                for (fourcc, rt_fmt) in VA_FORMAT_MAPPING.iter() {
                    // Should be rare.
                    if !vpp_pixel_formats.contains(fourcc) {
                        continue;
                    }

                    // Have to create a surface to query the format.
                    let mut surfaces = unsafe { vec![std::mem::zeroed::<VASurfaceID>(); 1] };
                    let mut status = unsafe {
                        (va.vaCreateSurfaces)(
                            display,
                            *rt_fmt,
                            1920,
                            1080,
                            surfaces.as_mut_ptr(),
                            1,
                            std::ptr::null_mut(),
                            0,
                        )
                    };

                    if status != VA_STATUS_SUCCESS {
                        log::warn!(
                            "vaCreateSurfaces failed: {}",
                            Self::va_status_to_string(&va, status)
                        );
                        continue;
                    }

                    // query tonemap capabilities.
                    let mut ctx_id: VAContextID = 0;
                    status = unsafe {
                        (va.vaCreateContext)(
                            display,
                            config_id,
                            1920,
                            1080,
                            VA_PROGRESSIVE,
                            surfaces.as_mut_ptr(),
                            1,
                            &mut ctx_id,
                        )
                    };

                    if status == VA_STATUS_SUCCESS {
                        if can_overlay.is_none() {
                            log::trace!("querying VAAPI proc pipeline capabilities");

                            let mut proc_caps = unsafe { std::mem::zeroed::<VAProcPipelineCaps>() };

                            let status = unsafe {
                                (va.vaQueryVideoProcPipelineCaps)(
                                    display,
                                    ctx_id,
                                    std::ptr::null_mut(),
                                    0,
                                    &mut proc_caps,
                                )
                            };

                            if status == VA_STATUS_SUCCESS {
                                can_overlay = Some(proc_caps.blend_flags > 0);
                                log::trace!("can_overlay: {can_overlay:?}");
                            }
                        }

                        let mut num_filter_caps: c_uint = 2;
                        let mut filter_caps: Vec<VAProcFilterCapHighDynamicRange> = unsafe {
                            vec![
                                std::mem::zeroed::<VAProcFilterCapHighDynamicRange>();
                                num_filter_caps as usize
                            ]
                        };

                        log::trace!(
                            "querying VAAPI HDR tonemap capabilities for {}.",
                            String::from_utf8_lossy(&fourcc.to_ne_bytes())
                        );
                        // check tonemap
                        let mut status = unsafe {
                            (va.vaQueryVideoProcFilterCaps)(
                                display,
                                ctx_id,
                                VA_PROC_FILTER_HIGH_DYNAMIC_RANGE_MAPPING,
                                filter_caps.as_mut_ptr(),
                                &mut num_filter_caps,
                            )
                        };

                        if status == VA_STATUS_SUCCESS {
                            let returned_caps = &filter_caps[..num_filter_caps as usize];
                            let has_caps = returned_caps.iter().any(|cap| {
                                cap.metadata_type != VA_PROC_HIGH_DYNAMIC_RANGE_METADATA_NONE
                            });

                            log::trace!("VAAPI HDR tonemap capabilities: {:?}", returned_caps);

                            if has_caps {
                                if returned_caps.iter().any(|cap| {
                                    (VA_TONE_MAPPING_HDR_TO_HDR & (cap.caps_flag as i32)) > 0
                                }) {
                                    can_hdr_to_hdr_tonemap.insert(*fourcc);
                                }

                                if returned_caps.iter().any(|cap| {
                                    (VA_TONE_MAPPING_HDR_TO_SDR & (cap.caps_flag as i32)) > 0
                                }) {
                                    can_hdr_to_sdr_tonemap.insert(*fourcc);
                                }
                            } else {
                                log::debug!("no VAAPI HDR tonemap capabilities found.")
                            }
                        } else {
                            log::warn!(
                                "vaQueryVideoProcFilterCaps failed: {}",
                                Self::va_status_to_string(&va, status)
                            );
                        }

                        status = unsafe { (va.vaDestroyContext)(display, ctx_id) };
                        if status != VA_STATUS_SUCCESS {
                            log::warn!(
                                "vaDestroyContext failed: {}",
                                Self::va_status_to_string(&va, status)
                            );
                        }
                    }

                    unsafe { (va.vaDestroySurfaces)(display, surfaces.as_mut_ptr(), 1) };
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
            can_hdr_to_hdr_tonemap,
            can_hdr_to_sdr_tonemap,
            can_overlay: can_overlay.unwrap_or_default(),
            rate_control,
        })
    }

    fn va_status_to_string(va: &VaLib, status: VAStatus) -> String {
        unsafe {
            let err_str = (va.vaErrorStr)(status);
            if err_str.is_null() {
                String::new()
            } else {
                CStr::from_ptr(err_str).to_string_lossy().into_owned()
            }
        }
    }
}
