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
pub const kCMVideoCodecType_MPEG2Video: u32 = u32::from_be_bytes(*b"mp2v");

// frame_mbs_only_flag = 0 marks the stream as interlaced
const INTERLACED_H264_SPS: &[u8] = &[
    0x67, 0x64, 0x00, 0x28, 0xac, 0xd9, 0x40, 0x78, 0x04, 0x4f, 0xde, 0x02, 0x20, 0x00, 0x00, 0x03,
    0x00, 0x20, 0x00, 0x00, 0x07, 0x83, 0xe2, 0xc5, 0xb2, 0xc0,
];
const INTERLACED_H264_PPS: &[u8] = &[0x68, 0xfe, 0xbc, 0xb0];

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
    fn VTDecompressionSessionCreate(
        allocator: core_foundation::base::CFAllocatorRef,
        video_format_description: core_foundation::base::CFTypeRef,
        video_decoder_specification: core_foundation::dictionary::CFDictionaryRef,
        destination_image_buffer_attributes: core_foundation::dictionary::CFDictionaryRef,
        output_callback: *const std::ffi::c_void,
        decompression_session_out: *mut core_foundation::base::CFTypeRef,
    ) -> i32;
    fn VTDecompressionSessionInvalidate(session: core_foundation::base::CFTypeRef);
    static kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder:
        core_foundation::string::CFStringRef;
}

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMVideoFormatDescriptionCreateFromH264ParameterSets(
        allocator: core_foundation::base::CFAllocatorRef,
        parameter_set_count: usize,
        parameter_set_pointers: *const *const u8,
        parameter_set_sizes: *const usize,
        nal_unit_header_length: i32,
        format_description_out: *mut core_foundation::base::CFTypeRef,
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

/// Apple Silicon reports H.264 hardware decode, but session create fails for
/// interlaced H.264. MPEG-2 sessions take no sequence header, so interlacing
/// cannot be probed.
pub fn is_hardware_interlaced_decode_supported(codec_type: u32) -> bool {
    match codec_type {
        kCMVideoCodecType_H264 => {
            can_create_hardware_session(&[INTERLACED_H264_SPS, INTERLACED_H264_PPS])
        }
        kCMVideoCodecType_MPEG2Video => is_hardware_decode_supported(codec_type),
        _ => false,
    }
}

fn can_create_hardware_session(h264_parameter_sets: &[&[u8]]) -> bool {
    let pointers: Vec<*const u8> = h264_parameter_sets.iter().map(|p| p.as_ptr()).collect();
    let sizes: Vec<usize> = h264_parameter_sets.iter().map(|p| p.len()).collect();

    let mut format_description: core_foundation::base::CFTypeRef = ptr::null();
    let status = unsafe {
        CMVideoFormatDescriptionCreateFromH264ParameterSets(
            ptr::null(),
            pointers.len(),
            pointers.as_ptr(),
            sizes.as_ptr(),
            4,
            &mut format_description,
        )
    };
    if status != 0 || format_description.is_null() {
        return false;
    }
    let format_description = unsafe { CFType::wrap_under_create_rule(format_description) };

    let specification = CFDictionary::from_CFType_pairs(&[(
        unsafe {
            CFString::wrap_under_get_rule(
                kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder,
            )
        },
        CFBoolean::true_value(),
    )]);

    let mut session: core_foundation::base::CFTypeRef = ptr::null();
    let status = unsafe {
        VTDecompressionSessionCreate(
            ptr::null(),
            format_description.as_CFTypeRef(),
            specification.as_concrete_TypeRef(),
            ptr::null(),
            ptr::null(),
            &mut session,
        )
    };
    if status != 0 || session.is_null() {
        return false;
    }

    unsafe {
        VTDecompressionSessionInvalidate(session);
        CFType::wrap_under_create_rule(session);
    }
    true
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
        kCMVideoCodecType_MPEG2Video => "MPEG-2",
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
        assert_eq!(kCMVideoCodecType_MPEG2Video, 0x6d703276);
    }

    #[test]
    fn codec_type_name_known() {
        assert_eq!(codec_type_name(kCMVideoCodecType_H264), "H.264");
        assert_eq!(codec_type_name(kCMVideoCodecType_HEVC), "HEVC");
        assert_eq!(codec_type_name(kCMVideoCodecType_VP9), "VP9");
        assert_eq!(codec_type_name(kCMVideoCodecType_AV1), "AV1");
        assert_eq!(codec_type_name(kCMVideoCodecType_MPEG2Video), "MPEG-2");
        assert_eq!(codec_type_name(0x00000000), "Other");
    }

    /// Catches a broken probe that rejects all streams. Ignored: needs H.264
    /// decode hardware.
    #[test]
    #[ignore]
    #[cfg(target_os = "macos")]
    fn hardware_session_created_for_progressive_h264() {
        const SPS: &[u8] = &[
            0x67, 0x64, 0x00, 0x28, 0xac, 0xd9, 0x40, 0x78, 0x02, 0x27, 0xe5, 0xc0, 0x44, 0x00,
            0x00, 0x03, 0x00, 0x04, 0x00, 0x00, 0x03, 0x00, 0xf0, 0x3c, 0x60, 0xc6, 0x58,
        ];
        const PPS: &[u8] = &[0x68, 0xeb, 0xe3, 0xcb, 0x22, 0xc0];

        assert!(can_create_hardware_session(&[SPS, PPS]));
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
            kCMVideoCodecType_MPEG2Video,
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

        println!("\n=== VideoToolbox Hardware Interlaced Decode Support ===");
        for codec_type in [kCMVideoCodecType_H264, kCMVideoCodecType_MPEG2Video] {
            let supported = is_hardware_interlaced_decode_supported(codec_type);
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
