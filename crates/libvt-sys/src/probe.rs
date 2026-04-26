use std::ptr;

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;

// CMVideoCodecType constants (FourCharCode / u32)
pub const kCMVideoCodecType_H264: u32 = u32::from_be_bytes(*b"avc1");
pub const kCMVideoCodecType_HEVC: u32 = u32::from_be_bytes(*b"hvc1");
pub const kCMVideoCodecType_VP9: u32 = u32::from_be_bytes(*b"vp09");
pub const kCMVideoCodecType_AV1: u32 = u32::from_be_bytes(*b"av01");

// VTVideoEncoderList dictionary keys
const ENCODER_LIST_CODEC_TYPE: &str = "CodecType";
const ENCODER_LIST_IS_HW_ACCELERATED: &str = "IsHardwareAccelerated";

#[link(name = "VideoToolbox", kind = "framework")]
unsafe extern "C" {
    fn VTRegisterSupplementalVideoDecoderIfAvailable(codec_type: u32);
    fn VTIsHardwareDecodeSupported(codec_type: u32) -> u8;
    fn VTCopyVideoEncoderList(
        options: *const core_foundation::base::CFTypeRef,
        list_of_video_encoders_out: *mut core_foundation::base::CFTypeRef,
    ) -> i32;
}

/// Returns true if the given codec type has hardware decode support.
pub fn is_hardware_decode_supported(codec_type: u32) -> bool {
    unsafe {
        if codec_type == kCMVideoCodecType_VP9 || codec_type == kCMVideoCodecType_AV1 {
            VTRegisterSupplementalVideoDecoderIfAvailable(codec_type);
        }
        VTIsHardwareDecodeSupported(codec_type) != 0
    }
}

/// Returns the FourCC string for a codec type (e.g. 0x61766331 -> "avc1").
pub fn codec_type_fourcc(codec_type: u32) -> [u8; 4] {
    codec_type.to_be_bytes()
}

/// Returns the codec type name for display purposes.
pub fn codec_type_name(codec_type: u32) -> &'static str {
    match codec_type {
        kCMVideoCodecType_H264 => "H.264",
        kCMVideoCodecType_HEVC => "HEVC",
        kCMVideoCodecType_VP9 => "VP9",
        kCMVideoCodecType_AV1 => "AV1",
        _ => "Other",
    }
}

/// Returns a list of codec types that have hardware-accelerated encoders.
pub fn hardware_encoder_codec_types() -> Vec<u32> {
    let mut list_ref: core_foundation::base::CFTypeRef = ptr::null_mut();
    let status = unsafe { VTCopyVideoEncoderList(ptr::null(), &mut list_ref) };

    if status != 0 || list_ref.is_null() {
        return Vec::new();
    }

    let array: CFArray<CFType> =
        unsafe { CFArray::wrap_under_create_rule(list_ref as core_foundation::array::CFArrayRef) };

    let mut hw_codec_types = Vec::new();

    for i in 0..array.len() {
        let Some(entry_ref) = array.get(i) else {
            continue;
        };
        let dict: CFDictionary<CFString, CFType> = unsafe {
            CFDictionary::wrap_under_get_rule(
                entry_ref.as_CFTypeRef() as core_foundation::dictionary::CFDictionaryRef
            )
        };

        let is_hw = dict
            .find(CFString::new(ENCODER_LIST_IS_HW_ACCELERATED))
            .map(|val| {
                let bool_ref = unsafe {
                    CFBoolean::wrap_under_get_rule(
                        val.as_CFTypeRef() as core_foundation::boolean::CFBooleanRef
                    )
                };
                bool_ref == CFBoolean::true_value()
            })
            .unwrap_or(false);

        if !is_hw {
            continue;
        }

        if let Some(codec_type_val) = dict.find(CFString::new(ENCODER_LIST_CODEC_TYPE)) {
            let num = unsafe {
                CFNumber::wrap_under_get_rule(
                    codec_type_val.as_CFTypeRef() as core_foundation::number::CFNumberRef
                )
            };
            if let Some(val) = num.to_i64() {
                hw_codec_types.push(val as u32);
            }
        }
    }

    hw_codec_types
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_type_constants_match_fourcc() {
        // 'avc1' = 0x61766331
        assert_eq!(kCMVideoCodecType_H264, 0x61766331);
        // 'hvc1' = 0x68766331
        assert_eq!(kCMVideoCodecType_HEVC, 0x68766331);
        // 'vp09' = 0x76703039
        assert_eq!(kCMVideoCodecType_VP9, 0x76703039);
        // 'av01' = 0x61763031
        assert_eq!(kCMVideoCodecType_AV1, 0x61763031);
    }

    #[test]
    fn codec_type_name_known() {
        assert_eq!(codec_type_name(kCMVideoCodecType_H264), "H.264");
        assert_eq!(codec_type_name(kCMVideoCodecType_HEVC), "HEVC");
        assert_eq!(codec_type_name(kCMVideoCodecType_VP9), "VP9");
        assert_eq!(codec_type_name(kCMVideoCodecType_AV1), "AV1");
        assert_eq!(codec_type_name(0x00000000), "Other");
    }

    /// Run with: cargo test -p libvt-sys -- --ignored --nocapture
    #[test]
    #[ignore]
    #[cfg(target_os = "macos")]
    fn print_videotoolbox_capabilities() {
        let codecs = [
            kCMVideoCodecType_H264,
            kCMVideoCodecType_HEVC,
            kCMVideoCodecType_VP9,
            kCMVideoCodecType_AV1,
        ];

        println!("\n=== VideoToolbox Hardware Decode Support ===");
        for codec_type in codecs {
            let supported = is_hardware_decode_supported(codec_type);
            println!(
                "  {:<8} (0x{:08x}): {}",
                codec_type_name(codec_type),
                codec_type,
                if supported { "YES" } else { "no" }
            );
        }

        println!("\n=== VideoToolbox Hardware Encoders ===");
        let hw_encoders = hardware_encoder_codec_types();
        if hw_encoders.is_empty() {
            println!("  (none found)");
        } else {
            for codec_type in &hw_encoders {
                let fourcc = codec_type_fourcc(*codec_type);
                let fourcc_str = std::str::from_utf8(&fourcc).unwrap_or("????");
                println!(
                    "  {:<8} ('{}' / 0x{:08x})",
                    codec_type_name(*codec_type),
                    fourcc_str,
                    codec_type
                );
            }
        }
        println!();
    }
}
