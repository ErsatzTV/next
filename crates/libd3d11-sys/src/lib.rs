// The COM interfaces are mirrored as C structs with a vtable pointer, as libamf-sys does
// for AMF, so no Windows binding crate is needed. Only the slots that are called have
// real signatures; the others are opaque so the offsets still match.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use std::ffi::c_void;

#[cfg(target_os = "windows")]
mod ffi;
#[cfg(target_os = "windows")]
pub use ffi::{D3d11Device, Dx11Lib};

pub type HRESULT = i32;
pub const DXGI_ERROR_NOT_FOUND: HRESULT = 0x887A_0002_u32 as i32;

pub const fn succeeded(hr: HRESULT) -> bool {
    hr >= 0
}

/// AMF only initializes on AMD adapters
pub const VENDOR_ID_AMD: u32 = 0x1002;

pub const DXGI_ADAPTER_FLAG_SOFTWARE: u32 = 2;

pub const D3D_DRIVER_TYPE_UNKNOWN: u32 = 0;
pub const D3D11_CREATE_DEVICE_VIDEO_SUPPORT: u32 = 0x800;
pub const D3D11_SDK_VERSION: u32 = 7;

#[repr(C)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

pub const IID_IDXGIFactory1: GUID = GUID {
    data1: 0x770a_ae78,
    data2: 0xf26f,
    data3: 0x4dba,
    data4: [0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87],
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LUID {
    pub LowPart: u32,
    pub HighPart: i32,
}

impl LUID {
    /// Byte order matches `VkPhysicalDeviceIDProperties::deviceLUID`, so the two can be
    /// compared directly
    pub fn to_bytes(self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        bytes[..4].copy_from_slice(&self.LowPart.to_ne_bytes());
        bytes[4..].copy_from_slice(&self.HighPart.to_ne_bytes());
        bytes
    }
}

#[repr(C)]
pub struct DXGI_ADAPTER_DESC1 {
    pub Description: [u16; 128],
    pub VendorId: u32,
    pub DeviceId: u32,
    pub SubSysId: u32,
    pub Revision: u32,
    pub DedicatedVideoMemory: usize,
    pub DedicatedSystemMemory: usize,
    pub SharedSystemMemory: usize,
    pub AdapterLuid: LUID,
    pub Flags: u32,
}

pub type ComFn = unsafe extern "system" fn();

#[repr(C)]
pub struct IUnknownVtbl {
    pub QueryInterface: ComFn,
    pub AddRef: ComFn,
    pub Release: unsafe extern "system" fn(*mut IUnknown) -> u32,
}

#[repr(C)]
pub struct IUnknown {
    pub lpVtbl: *const IUnknownVtbl,
}

/// # Safety
/// `ptr` must be null or a live COM interface pointer.
pub unsafe fn release(ptr: *mut c_void) {
    if !ptr.is_null() {
        let unknown = ptr.cast::<IUnknown>();
        unsafe { ((*(*unknown).lpVtbl).Release)(unknown) };
    }
}

#[repr(C)]
pub struct IDXGIFactory1Vtbl {
    // IUnknown
    pub QueryInterface: ComFn,
    pub AddRef: ComFn,
    pub Release: unsafe extern "system" fn(*mut IUnknown) -> u32,
    // IDXGIObject
    pub SetPrivateData: ComFn,
    pub SetPrivateDataInterface: ComFn,
    pub GetPrivateData: ComFn,
    pub GetParent: ComFn,
    // IDXGIFactory
    pub EnumAdapters: ComFn,
    pub MakeWindowAssociation: ComFn,
    pub GetWindowAssociation: ComFn,
    pub CreateSwapChain: ComFn,
    pub CreateSoftwareAdapter: ComFn,
    // IDXGIFactory1
    pub EnumAdapters1:
        unsafe extern "system" fn(*mut IDXGIFactory1, u32, *mut *mut IDXGIAdapter1) -> HRESULT,
    pub IsCurrent: ComFn,
}

#[repr(C)]
pub struct IDXGIFactory1 {
    pub lpVtbl: *const IDXGIFactory1Vtbl,
}

#[repr(C)]
pub struct IDXGIAdapter1Vtbl {
    // IUnknown
    pub QueryInterface: ComFn,
    pub AddRef: ComFn,
    pub Release: unsafe extern "system" fn(*mut IUnknown) -> u32,
    // IDXGIObject
    pub SetPrivateData: ComFn,
    pub SetPrivateDataInterface: ComFn,
    pub GetPrivateData: ComFn,
    pub GetParent: ComFn,
    // IDXGIAdapter
    pub EnumOutputs: ComFn,
    pub GetDesc: ComFn,
    pub CheckInterfaceSupport: ComFn,
    // IDXGIAdapter1
    pub GetDesc1: unsafe extern "system" fn(*mut IDXGIAdapter1, *mut DXGI_ADAPTER_DESC1) -> HRESULT,
}

#[repr(C)]
pub struct IDXGIAdapter1 {
    pub lpVtbl: *const IDXGIAdapter1Vtbl,
}

/// `index` is the raw DXGI enumeration index, software adapters included, because that
/// is what ffmpeg's `d3d11va` device string selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInfo {
    pub index: u32,
    pub description: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub dedicated_video_memory: usize,
    pub luid: [u8; 8],
    pub flags: u32,
}

impl AdapterInfo {
    pub fn is_software(&self) -> bool {
        self.flags & DXGI_ADAPTER_FLAG_SOFTWARE != 0
    }
}

impl From<(u32, &DXGI_ADAPTER_DESC1)> for AdapterInfo {
    fn from((index, desc): (u32, &DXGI_ADAPTER_DESC1)) -> Self {
        let len = desc
            .Description
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(desc.Description.len());
        AdapterInfo {
            index,
            description: String::from_utf16_lossy(&desc.Description[..len]),
            vendor_id: desc.VendorId,
            device_id: desc.DeviceId,
            dedicated_video_memory: desc.DedicatedVideoMemory,
            luid: desc.AdapterLuid.to_bytes(),
            flags: desc.Flags,
        }
    }
}

#[cfg(all(target_os = "windows", target_pointer_width = "64"))]
mod layout {
    use std::mem::offset_of;

    use super::*;

    const _: () = assert!(size_of::<DXGI_ADAPTER_DESC1>() == 312);
    const _: () = assert!(offset_of!(DXGI_ADAPTER_DESC1, VendorId) == 256);
    const _: () = assert!(offset_of!(DXGI_ADAPTER_DESC1, DedicatedVideoMemory) == 272);
    const _: () = assert!(offset_of!(DXGI_ADAPTER_DESC1, AdapterLuid) == 296);
    const _: () = assert!(offset_of!(DXGI_ADAPTER_DESC1, Flags) == 304);
    const _: () = assert!(offset_of!(IDXGIFactory1Vtbl, EnumAdapters1) == 12 * 8);
    const _: () = assert!(offset_of!(IDXGIAdapter1Vtbl, GetDesc1) == 10 * 8);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_info_from_desc() {
        let mut desc = DXGI_ADAPTER_DESC1 {
            Description: [0; 128],
            VendorId: VENDOR_ID_AMD,
            DeviceId: 0x73ff,
            SubSysId: 0,
            Revision: 0,
            DedicatedVideoMemory: 8 << 30,
            DedicatedSystemMemory: 0,
            SharedSystemMemory: 0,
            AdapterLuid: LUID {
                LowPart: 0x0001_0203,
                HighPart: 0x0405_0607,
            },
            Flags: 0,
        };
        for (i, c) in "RX 6600M".encode_utf16().enumerate() {
            desc.Description[i] = c;
        }

        let info = AdapterInfo::from((1, &desc));
        assert_eq!(info.index, 1);
        assert_eq!(info.description, "RX 6600M");
        assert_eq!(info.vendor_id, VENDOR_ID_AMD);
        assert_eq!(info.device_id, 0x73ff);
        assert!(!info.is_software());

        let mut expected = [0u8; 8];
        expected[..4].copy_from_slice(&0x0001_0203_u32.to_ne_bytes());
        expected[4..].copy_from_slice(&0x0405_0607_i32.to_ne_bytes());
        assert_eq!(info.luid, expected);
    }

    #[test]
    fn software_flag() {
        let info = AdapterInfo {
            index: 2,
            description: String::from("Microsoft Basic Render Driver"),
            vendor_id: 0x1414,
            device_id: 0x8c,
            dedicated_video_memory: 0,
            luid: [0; 8],
            flags: DXGI_ADAPTER_FLAG_SOFTWARE,
        };
        assert!(info.is_software());
    }
}
