use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;

use libloading::Library;

use crate::{
    AdapterInfo, D3D_DRIVER_TYPE_UNKNOWN, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
    DXGI_ADAPTER_DESC1, DXGI_ERROR_NOT_FOUND, GUID, HRESULT, IDXGIAdapter1, IDXGIFactory1,
    IID_IDXGIFactory1, release, succeeded,
};

struct Dx11Modules {
    _dxgi: Library,
    _d3d11: Library,
}

pub struct Dx11Lib {
    modules: Arc<Dx11Modules>,
    pub CreateDXGIFactory1: unsafe extern "system" fn(*const GUID, *mut *mut c_void) -> HRESULT,
    pub D3D11CreateDevice: unsafe extern "system" fn(
        *mut IDXGIAdapter1,
        u32,
        *mut c_void,
        u32,
        *const u32,
        u32,
        u32,
        *mut *mut c_void,
        *mut u32,
        *mut *mut c_void,
    ) -> HRESULT,
}

pub struct D3d11Device {
    pointer: *mut c_void,
    // Drop releases COM before fields (and the final module owner) are dropped.
    _modules: Arc<Dx11Modules>,
}

impl D3d11Device {
    pub fn as_ptr(&self) -> *mut c_void {
        self.pointer
    }
}

impl Drop for D3d11Device {
    fn drop(&mut self) {
        unsafe { release(self.pointer) };
    }
}

struct Com {
    pointer: *mut c_void,
    modules: Arc<Dx11Modules>,
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { release(self.pointer) };
    }
}

impl Dx11Lib {
    pub fn load() -> Result<Self, libloading::Error> {
        unsafe {
            let dxgi = Library::new("dxgi.dll")?;
            let d3d11 = Library::new("d3d11.dll")?;
            let CreateDXGIFactory1 = *dxgi.get(b"CreateDXGIFactory1\0")?;
            let D3D11CreateDevice = *d3d11.get(b"D3D11CreateDevice\0")?;
            Ok(Self {
                modules: Arc::new(Dx11Modules {
                    _dxgi: dxgi,
                    _d3d11: d3d11,
                }),
                CreateDXGIFactory1,
                D3D11CreateDevice,
            })
        }
    }

    fn factory(&self) -> Result<Com, String> {
        let mut factory: *mut c_void = ptr::null_mut();
        let hr = unsafe { (self.CreateDXGIFactory1)(&IID_IDXGIFactory1, &mut factory) };
        if !succeeded(hr) || factory.is_null() {
            return Err(format!("CreateDXGIFactory1 failed: 0x{hr:08x}"));
        }
        Ok(Com {
            pointer: factory,
            modules: Arc::clone(&self.modules),
        })
    }

    unsafe fn enum_adapter(factory: &Com, index: u32) -> Result<Option<Com>, String> {
        let modules = Arc::clone(&factory.modules);
        let factory = factory.pointer.cast::<IDXGIFactory1>();
        let mut adapter: *mut IDXGIAdapter1 = ptr::null_mut();
        let hr = unsafe { ((*(*factory).lpVtbl).EnumAdapters1)(factory, index, &mut adapter) };
        if hr == DXGI_ERROR_NOT_FOUND {
            return Ok(None);
        }
        if !succeeded(hr) || adapter.is_null() {
            return Err(format!("EnumAdapters1({index}) failed: 0x{hr:08x}"));
        }
        Ok(Some(Com {
            pointer: adapter.cast(),
            modules,
        }))
    }

    pub fn enumerate_adapters(&self) -> Result<Vec<AdapterInfo>, String> {
        let factory = self.factory()?;
        let mut adapters = Vec::new();
        let mut index = 0;
        unsafe {
            while let Some(adapter) = Self::enum_adapter(&factory, index)? {
                let adapter_ptr = adapter.pointer.cast::<IDXGIAdapter1>();
                let mut desc: DXGI_ADAPTER_DESC1 = std::mem::zeroed();
                let hr = ((*(*adapter_ptr).lpVtbl).GetDesc1)(adapter_ptr, &mut desc);
                if !succeeded(hr) {
                    return Err(format!("GetDesc1({index}) failed: 0x{hr:08x}"));
                }
                adapters.push(AdapterInfo::from((index, &desc)));
                index += 1;
            }
        }
        Ok(adapters)
    }

    /// Uses the same creation flag as ffmpeg's `d3d11va` hwcontext, so AMF is probed on
    /// the same kind of device it will later run on.
    pub fn create_device(&self, index: u32) -> Result<D3d11Device, String> {
        let factory = self.factory()?;
        let adapter = unsafe { Self::enum_adapter(&factory, index)? }
            .ok_or_else(|| format!("adapter {index} not found"))?;

        let mut device: *mut c_void = ptr::null_mut();
        let hr = unsafe {
            (self.D3D11CreateDevice)(
                adapter.pointer.cast(),
                D3D_DRIVER_TYPE_UNKNOWN,
                ptr::null_mut(),
                D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                ptr::null(),
                0,
                D3D11_SDK_VERSION,
                &mut device,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if !succeeded(hr) || device.is_null() {
            return Err(format!(
                "D3D11CreateDevice on adapter {index} failed: 0x{hr:08x}"
            ));
        }
        Ok(D3d11Device {
            pointer: device,
            _modules: Arc::clone(&self.modules),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Use only kernel32 module references and a fake IUnknown. These regression
    // tests must not enumerate adapters, create a GPU device, or invoke AMF.
    #[repr(C)]
    struct FakeUnknown {
        unknown: crate::IUnknown,
        modules: std::sync::Weak<Dx11Modules>,
        released_with_modules: bool,
        releases: usize,
    }

    unsafe extern "system" fn unused_com_slot() {}

    unsafe extern "system" fn fake_release(pointer: *mut crate::IUnknown) -> u32 {
        let fake = unsafe { &mut *pointer.cast::<FakeUnknown>() };
        fake.released_with_modules = fake.modules.upgrade().is_some();
        fake.releases += 1;
        0
    }

    fn assert_release_before_modules<T>(make: impl FnOnce(*mut c_void, Arc<Dx11Modules>) -> T) {
        static VTABLE: crate::IUnknownVtbl = crate::IUnknownVtbl {
            QueryInterface: unused_com_slot,
            AddRef: unused_com_slot,
            Release: fake_release,
        };
        let modules = Arc::new(Dx11Modules {
            _dxgi: unsafe { Library::new("kernel32.dll") }.unwrap(),
            _d3d11: unsafe { Library::new("kernel32.dll") }.unwrap(),
        });
        let weak = Arc::downgrade(&modules);
        let mut fake = Box::new(FakeUnknown {
            unknown: crate::IUnknown { lpVtbl: &VTABLE },
            modules: weak.clone(),
            released_with_modules: false,
            releases: 0,
        });
        let owner = make((&mut fake.unknown as *mut crate::IUnknown).cast(), modules);
        assert_eq!(weak.strong_count(), 1);
        drop(owner);
        assert_eq!(fake.releases, 1);
        assert!(fake.released_with_modules);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn device_releases_com_before_unloading_modules() {
        assert_release_before_modules(|pointer, modules| D3d11Device {
            pointer,
            _modules: modules,
        });
    }

    #[test]
    fn factory_or_adapter_releases_com_before_unloading_modules() {
        assert_release_before_modules(|pointer, modules| Com { pointer, modules });
    }

    /// Needs a real display adapter, so it only runs with --ignored
    #[test]
    #[ignore]
    fn enumerate_and_create() {
        let dx = Dx11Lib::load().expect("load dxgi/d3d11");
        let adapters = dx.enumerate_adapters().expect("enumerate");
        assert!(!adapters.is_empty());
        for adapter in &adapters {
            println!("{adapter:?}");
            if !adapter.is_software() {
                let _device = dx.create_device(adapter.index).expect("create device");
            }
        }
    }
}
