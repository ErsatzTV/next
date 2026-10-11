#![allow(non_snake_case)]

use libloading::Library;

use crate::{AMF_RESULT, AMFFactory, amf_uint64};

pub struct AmfLib {
    _lib: Library,
    pub AMFInit: unsafe extern "C" fn(amf_uint64, *mut *mut AMFFactory) -> AMF_RESULT,
    pub AMFQueryVersion: unsafe extern "C" fn(*mut amf_uint64) -> AMF_RESULT,
}

impl AmfLib {
    pub fn load() -> Result<Self, libloading::Error> {
        // Linux only ships the runtime with the proprietary amdgpu-pro driver
        #[cfg(target_os = "linux")]
        let name = "libamfrt64.so.1";
        #[cfg(target_os = "windows")]
        let name = "amfrt64.dll";
        unsafe {
            let lib = Library::new(name)?;
            let AMFInit = *lib.get(b"AMFInit\0")?;
            let AMFQueryVersion = *lib.get(b"AMFQueryVersion\0")?;
            Ok(Self {
                _lib: lib,
                AMFInit,
                AMFQueryVersion,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::*;
    use crate::{
        AMF_DX11_1, AMF_OK, AMFVideoEncoderVCE_AVC, AmfInterface, amf_result_name,
        amf_surface_format_name, amf_version_parts, wide,
    };

    /// Needs an AMD GPU with the AMF runtime installed, so it only runs with --ignored
    #[test]
    #[ignore]
    fn create_context_and_query_encoder_caps() {
        let amf = match AmfLib::load() {
            Ok(amf) => amf,
            Err(err) => {
                println!("failed to load AMF runtime: {err}");
                return;
            }
        };

        unsafe {
            let mut version: amf_uint64 = 0;
            assert_eq!((amf.AMFQueryVersion)(&mut version), AMF_OK);
            let (major, minor, release, build) = amf_version_parts(version);
            println!("runtime version: {major}.{minor}.{release}.{build}");

            let mut factory = ptr::null_mut();
            let result = (amf.AMFInit)(version, &mut factory);
            assert_eq!(result, AMF_OK, "AMFInit: {}", amf_result_name(result));
            assert!(!factory.is_null());

            let mut context = ptr::null_mut();
            let result = ((*(*factory).pVtbl).CreateContext)(factory, &mut context);
            assert_eq!(result, AMF_OK, "CreateContext: {}", amf_result_name(result));

            #[cfg(target_os = "windows")]
            {
                let result = ((*(*context).pVtbl).InitDX11)(context, ptr::null_mut(), AMF_DX11_1);
                assert_eq!(result, AMF_OK, "InitDX11: {}", amf_result_name(result));
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = AMF_DX11_1;
                let mut context1: *mut crate::AMFContext1 = ptr::null_mut();
                let result = ((*(*context).pVtbl).QueryInterface)(
                    context,
                    &crate::IID_AMFContext1,
                    (&raw mut context1).cast(),
                );
                assert_eq!(
                    result,
                    AMF_OK,
                    "QueryInterface: {}",
                    amf_result_name(result)
                );
                let result = ((*(*context1).pVtbl).InitVulkan)(context1, ptr::null_mut());
                crate::AMFContext1::release(context1);
                assert_eq!(result, AMF_OK, "InitVulkan: {}", amf_result_name(result));
            }

            let id = wide(AMFVideoEncoderVCE_AVC);
            let mut encoder = ptr::null_mut();
            let result =
                ((*(*factory).pVtbl).CreateComponent)(factory, context, id.as_ptr(), &mut encoder);
            println!("CreateComponent(AVC encoder): {}", amf_result_name(result));

            if result == AMF_OK {
                let mut caps = ptr::null_mut();
                let result = ((*(*encoder).pVtbl).GetCaps)(encoder, &mut caps);
                assert_eq!(result, AMF_OK, "GetCaps: {}", amf_result_name(result));

                let accel = ((*(*caps).pVtbl).GetAccelerationType)(caps);
                println!("acceleration type: {accel}");

                let mut input = ptr::null_mut();
                assert_eq!(((*(*caps).pVtbl).GetInputCaps)(caps, &mut input), AMF_OK);
                let count = ((*(*input).pVtbl).GetNumOfFormats)(input);
                for i in 0..count {
                    let mut format = 0;
                    let mut native = 0;
                    if ((*(*input).pVtbl).GetFormatAt)(input, i, &mut format, &mut native) == AMF_OK
                    {
                        println!(
                            "input format {i}: {} native={}",
                            amf_surface_format_name(format),
                            native != 0
                        );
                    }
                }
                crate::AMFIOCaps::release(input);
                crate::AMFCaps::release(caps);
                crate::AMFComponent::release(encoder);
            }

            ((*(*context).pVtbl).Terminate)(context);
            crate::AMFContext::release(context);
        }
    }
}
