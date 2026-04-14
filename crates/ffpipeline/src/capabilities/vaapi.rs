use std::collections::HashSet;
use std::ffi::CStr;
use std::fs::File;
use std::os::unix::io::AsRawFd;

use libva_sys::*;

use crate::error::FFPipelineError;
use crate::error::FFPipelineError::VaapiCapabilitiesError;

#[derive(Debug, Clone)]
pub struct VaapiCapabilities {
    vendor: String,
    supported: HashSet<(VAProfile, VAEntrypoint)>,
}

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
        let file = File::options()
            .read(true)
            .write(true)
            .open(device)
            .map_err(|e| VaapiCapabilitiesError(format!("cannot open {device}: {e}")))?;

        let display = unsafe { vaGetDisplayDRM(file.as_raw_fd()) };
        if display.is_null() {
            return Err(VaapiCapabilitiesError(String::from(
                "vaGetDisplayDRM returned NULL",
            )));
        }

        let mut major = 0i32;
        let mut minor = 0i32;
        let status = unsafe { vaInitialize(display, &mut major, &mut minor) };
        if status != VA_STATUS_SUCCESS {
            return Err(VaapiCapabilitiesError(format!(
                "vaInitialize failed: {status}"
            )));
        }

        let vendor = unsafe {
            let ptr = vaQueryVendorString(display);
            if ptr.is_null() {
                String::new()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        };

        let max_profiles = unsafe { vaMaxNumProfiles(display) } as usize;
        let mut profiles = vec![0i32; max_profiles];
        let mut num_profiles = 0i32;
        let status =
            unsafe { vaQueryConfigProfiles(display, profiles.as_mut_ptr(), &mut num_profiles) };
        if status != VA_STATUS_SUCCESS {
            unsafe {
                vaTerminate(display);
            }
            return Err(VaapiCapabilitiesError(format!(
                "vaQueryConfigProfiles failed: {status}"
            )));
        }
        profiles.truncate(num_profiles as usize);

        let max_entrypoints = unsafe { vaMaxNumEntrypoints(display) } as usize;
        let mut supported = HashSet::new();

        for &profile in &profiles {
            let mut entrypoints = vec![0i32; max_entrypoints];
            let mut num_entrypoints = 0i32;
            let status = unsafe {
                vaQueryConfigEntrypoints(
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

        unsafe {
            vaTerminate(display);
        }

        Ok(VaapiCapabilities { vendor, supported })
    }

    pub fn can_decode(&self, codec: &str, profile: &str, bit_depth: u8) -> bool {
        Self::decode_profile_for(codec, profile, bit_depth)
            .iter()
            .any(|p| self.supported.contains(&(*p, VA_ENTRYPOINT_VLD)))
    }

    fn decode_profile_for(codec: &str, profile: &str, _bit_depth: u8) -> Option<VAProfile> {
        match (codec, profile) {
            ("h264", "main" | "77") => Some(VA_PROFILE_H264_MAIN),
            ("h264", "high" | "100" | "high 10" | "110") => Some(VA_PROFILE_H264_HIGH),
            ("h264", "baseline constrained" | "constrained baseline" | "578") => {
                Some(VA_PROFILE_H264_CONSTRAINED_BASELINE)
            }
            ("mpeg2video", "main" | "4") => Some(VA_PROFILE_MPEG2_MAIN),
            ("mpeg2video", "simple" | "5") => Some(VA_PROFILE_MPEG2_SIMPLE),
            ("vc1", "simple" | "0") => Some(VA_PROFILE_VC1_SIMPLE),
            ("vc1", "main" | "1") => Some(VA_PROFILE_VC1_MAIN),
            ("vc1", "advanced" | "3") => Some(VA_PROFILE_VC1_ADVANCED),
            ("hevc", "main" | "1") => Some(VA_PROFILE_HEVC_MAIN),
            ("hevc", "main 10" | "2") => Some(VA_PROFILE_HEVC_MAIN10),
            ("vp9", "profile 0" | "0") => Some(VA_PROFILE_VP9_PROFILE0),
            ("vp9", "profile 1" | "1") => Some(VA_PROFILE_VP9_PROFILE1),
            ("vp9", "profile 2" | "2") => Some(VA_PROFILE_VP9_PROFILE2),
            ("vp9", "profile 3" | "3") => Some(VA_PROFILE_VP9_PROFILE3),
            ("av1", "main" | "0") => Some(VA_PROFILE_AV1_PROFILE0),
            _ => None,
        }
    }

    pub fn vendor(&self) -> &str {
        &self.vendor
    }

    pub fn count(&self) -> usize {
        self.supported.len()
    }
}
