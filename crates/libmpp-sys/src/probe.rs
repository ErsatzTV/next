use libloading::Library;

pub const MPP_OK: i32 = 0;

pub const MPP_CTX_DEC: i32 = 0;
pub const MPP_CTX_ENC: i32 = 1;

pub const MPP_VIDEO_CodingAVC: i32 = 7;
pub const MPP_VIDEO_CodingVP8: i32 = 9;
pub const MPP_VIDEO_CodingVP9: i32 = 10;
pub const MPP_VIDEO_CodingHEVC: i32 = 0x0100_0004;

pub struct MppLib {
    _lib: Library,
    mpp_check_support_format: unsafe extern "C" fn(ctx_type: i32, coding: i32) -> i32,
}

impl MppLib {
    pub fn load() -> Result<Self, libloading::Error> {
        let name = "librockchip_mpp.so.1";
        unsafe {
            let lib = Library::new(name)?;
            let mpp_check_support_format = *lib.get(b"mpp_check_support_format\0")?;
            Ok(Self {
                _lib: lib,
                mpp_check_support_format,
            })
        }
    }

    pub fn is_supported(&self, ctx_type: i32, coding: i32) -> bool {
        unsafe { (self.mpp_check_support_format)(ctx_type, coding) == MPP_OK }
    }
}
