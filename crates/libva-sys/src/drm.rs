#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int};
use std::os::fd::RawFd;

pub const AMDGPU_HW_IP_VCE: u32 = 4;
pub const AMDGPU_HW_IP_VCN_ENC: u32 = 7;

const AMDGPU_INFO_HW_IP_COUNT: u32 = 0x03;

#[repr(C)]
struct drm_version {
    version_major: c_int,
    version_minor: c_int,
    version_patchlevel: c_int,
    name_len: usize,
    name: *mut c_char,
    date_len: usize,
    date: *mut c_char,
    desc_len: usize,
    desc: *mut c_char,
}

#[repr(C)]
struct drm_amdgpu_info {
    return_pointer: u64,
    return_size: u32,
    query: u32,
    query_hw_ip_type: u32,
    query_hw_ip_instance: u32,
    // pad to the kernel's 16-byte union
    _union_pad: [u32; 2],
}

const _: () = assert!(size_of::<drm_amdgpu_info>() == 32);

const fn drm_iowr(nr: u32, size: usize) -> libc::Ioctl {
    ((3 << 30) | ((size as u32) << 16) | ((b'd' as u32) << 8) | nr) as libc::Ioctl
}

const fn drm_iow(nr: u32, size: usize) -> libc::Ioctl {
    ((1 << 30) | ((size as u32) << 16) | ((b'd' as u32) << 8) | nr) as libc::Ioctl
}

const DRM_IOCTL_VERSION: libc::Ioctl = drm_iowr(0x00, size_of::<drm_version>());
// DRM_COMMAND_BASE + DRM_AMDGPU_INFO
const DRM_IOCTL_AMDGPU_INFO: libc::Ioctl = drm_iow(0x45, size_of::<drm_amdgpu_info>());

pub fn drm_driver_name(fd: RawFd) -> Option<String> {
    let mut name = [0u8; 32];
    let mut version = drm_version {
        version_major: 0,
        version_minor: 0,
        version_patchlevel: 0,
        name_len: name.len(),
        name: name.as_mut_ptr().cast(),
        date_len: 0,
        date: std::ptr::null_mut(),
        desc_len: 0,
        desc: std::ptr::null_mut(),
    };
    if unsafe { libc::ioctl(fd, DRM_IOCTL_VERSION, &mut version) } != 0 {
        return None;
    }
    let len = version.name_len.min(name.len());
    Some(String::from_utf8_lossy(&name[..len]).into_owned())
}

/// Newer kernels return EINVAL when the chip has no such block, so an error means 0.
pub fn amdgpu_hw_ip_count(fd: RawFd, hw_ip_type: u32) -> u32 {
    let mut count = 0u32;
    let mut info = drm_amdgpu_info {
        return_pointer: &mut count as *mut u32 as u64,
        return_size: size_of::<u32>() as u32,
        query: AMDGPU_INFO_HW_IP_COUNT,
        query_hw_ip_type: hw_ip_type,
        query_hw_ip_instance: 0,
        _union_pad: [0; 2],
    };
    if unsafe { libc::ioctl(fd, DRM_IOCTL_AMDGPU_INFO, &mut info) } != 0 {
        return 0;
    }
    count
}
