use std::collections::{HashMap, HashSet};

use libvpl_sys::*;

use crate::capabilities::qsv::{QsvCapabilities, QsvFourCC, legacy};
use crate::error::FFPipelineError;
use crate::pipeline::VideoFormat;

// byte offsets of dec and enc inside mfxImplDescription (API 2.x, x86_64)
const IMPL_DESC_API_VERSION_OFFSET: usize = 12;
const IMPL_DESC_DEC_OFFSET: usize = 472;
const IMPL_DESC_ENC_OFFSET: usize = 504;
const IMPL_DESC_VPP_OFFSET: usize = 536;

impl QsvCapabilities {
    pub fn probe() -> Result<QsvCapabilities, FFPipelineError> {
        let mut supported_decoders: HashMap<VideoFormat, Vec<u8>> = HashMap::new();
        let mut supported_encoders: HashMap<VideoFormat, Vec<u8>> = HashMap::new();
        let mut tree = FilterTree::default();
        let mut runtime_api = None;

        let vpl = VplLib::load()
            .map_err(|e| FFPipelineError::QsvCapabilitiesError(format!("libvpl not found: {e}")))?;

        unsafe {
            let loader = (vpl.MFXLoad)();
            if loader.is_null() {
                return Err(FFPipelineError::QsvCapabilitiesError(
                    "MFXLoad failed".into(),
                ));
            }

            // filter for hardware implementations only
            let config = (vpl.MFXCreateConfig)(loader);
            if !config.is_null() {
                let variant = mfxVariant {
                    Version: 0,
                    Type: MFX_VARIANT_TYPE_U32,
                    Data: mfxVariantValue {
                        U32: MFX_IMPL_TYPE_HARDWARE,
                    },
                };
                let name = b"mfxImplDescription.Impl\0";
                (vpl.MFXSetConfigFilterProperty)(config, name.as_ptr(), variant);
            }

            // request the implementation description struct from the first matching impl
            let mut hdl: mfxHDL = std::ptr::null_mut();
            let status =
                (vpl.MFXEnumImplementations)(loader, 0, MFX_IMPLCAPS_IMPLDESCSTRUCTURE, &mut hdl);

            if status == MFX_ERR_NONE && !hdl.is_null() {
                // dec and enc are embedded at fixed offsets inside mfxImplDescription
                // we access them directly by byte offset to avoid defining the full 648-byte struct
                let base = hdl as *const u8;
                let api = &*(base.add(IMPL_DESC_API_VERSION_OFFSET) as *const mfxVersion);
                runtime_api = Some((api.Major, api.Minor));
                let dec = &*(base.add(IMPL_DESC_DEC_OFFSET) as *const mfxDecoderDescription);
                let enc = &*(base.add(IMPL_DESC_ENC_OFFSET) as *const mfxEncoderDescription);
                let vpp = &*(base.add(IMPL_DESC_VPP_OFFSET) as *const mfxVPPDescription);

                for &(format, codec_id) in legacy::CODECS {
                    if decoder_has_codec(dec, codec_id) {
                        if decoder_has_10bit_profile(dec, codec_id) {
                            supported_decoders.insert(format, vec![8u8, 10u8]);
                        } else {
                            supported_decoders.insert(format, vec![8u8]);
                        }
                    }

                    let enc_profiles = encoder_profiles(enc, codec_id);
                    if !enc_profiles.is_empty() {
                        if encoder_supports_10bit(&enc_profiles, codec_id) {
                            supported_encoders.insert(format, vec![8u8, 10u8]);
                        } else {
                            supported_encoders.insert(format, vec![8u8]);
                        }
                    }
                }

                tree = walk_filters(vpp);

                (vpl.MFXDispReleaseImplDescription)(loader, hdl);
            }

            (vpl.MFXUnload)(loader);
        }

        // the dispatcher reports a legacy runtime with an empty capability tree,
        // because MFXQueryImplsDescription exists only in API 2.x runtimes
        if supported_decoders.is_empty() && supported_encoders.is_empty() {
            log::debug!(
                "[qsv] VPL reported no codecs; falling back to a legacy Media SDK capability query"
            );

            // no codecs means no device. an error makes the channel use software
            return legacy::probe(&vpl).ok_or_else(|| {
                FFPipelineError::QsvCapabilitiesError("no hardware implementation found".into())
            });
        }

        #[cfg(target_os = "linux")]
        retain_va_decoders(&mut supported_decoders);

        Ok(QsvCapabilities {
            supported_decoders,
            supported_encoders,
            upload_formats: tree.pixel_formats,
            convert_pairs: tree.convert_pairs,
            vpp_filters: tree.filters,
            rotation_formats: tree.rotation_formats,
            composite_pairs: tree.composite_pairs,
            runtime_api,
        })
    }
}

/// VPL lists decoders the VA driver has disabled (e.g. VC-1 and VP8 on DG2,
/// intel/media-driver#1658); those fail at decoder init.
#[cfg(target_os = "linux")]
fn retain_va_decoders(decoders: &mut HashMap<VideoFormat, Vec<u8>>) {
    let Some(profiles) = legacy::display::VaDisplay::open().and_then(|d| d.decode_profiles())
    else {
        log::debug!("[qsv] no VA decode profiles; keeping the VPL decoder list");
        return;
    };

    filter_decoders(decoders, &profiles);
}

#[cfg(target_os = "linux")]
fn filter_decoders(
    decoders: &mut HashMap<VideoFormat, Vec<u8>>,
    profiles: &HashSet<libva_sys::VAProfile>,
) {
    use libva_sys::*;

    const VA_DECODE_PROFILES: &[(VideoFormat, VAProfile, u8)] = &[
        (VideoFormat::Av1, VA_PROFILE_AV1_PROFILE0, 8),
        (VideoFormat::Av1, VA_PROFILE_AV1_PROFILE0, 10),
        (VideoFormat::H264, VA_PROFILE_H264_CONSTRAINED_BASELINE, 8),
        (VideoFormat::H264, VA_PROFILE_H264_MAIN, 8),
        (VideoFormat::H264, VA_PROFILE_H264_HIGH, 8),
        (VideoFormat::H264, VA_PROFILE_H264_HIGH10, 10),
        (VideoFormat::Hevc, VA_PROFILE_HEVC_MAIN, 8),
        (VideoFormat::Hevc, VA_PROFILE_HEVC_MAIN10, 10),
        (VideoFormat::Mpeg2Video, VA_PROFILE_MPEG2_SIMPLE, 8),
        (VideoFormat::Mpeg2Video, VA_PROFILE_MPEG2_MAIN, 8),
        (VideoFormat::Vc1, VA_PROFILE_VC1_SIMPLE, 8),
        (VideoFormat::Vc1, VA_PROFILE_VC1_MAIN, 8),
        (VideoFormat::Vc1, VA_PROFILE_VC1_ADVANCED, 8),
        (VideoFormat::Vp8, VA_PROFILE_VP8_VERSION0_3, 8),
        (VideoFormat::Vp9, VA_PROFILE_VP9_PROFILE0, 8),
        (VideoFormat::Vp9, VA_PROFILE_VP9_PROFILE2, 10),
    ];

    decoders.retain(|format, depths| {
        depths.retain(|depth| {
            VA_DECODE_PROFILES
                .iter()
                .any(|(f, profile, d)| f == format && d == depth && profiles.contains(profile))
        });
        if depths.is_empty() {
            log::debug!("[qsv] VA driver has no decode profile for {format}; dropping it");
        }
        !depths.is_empty()
    });
}

/// Returns true if the decoder description lists the given codec.
unsafe fn decoder_has_codec(dec: &mfxDecoderDescription, codec_id: u32) -> bool {
    if dec.NumCodecs == 0 || dec.Codecs.is_null() {
        return false;
    }
    for i in 0..dec.NumCodecs as usize {
        let entry = unsafe { &*dec.Codecs.add(i) };
        if entry.CodecID == codec_id {
            return true;
        }
    }
    false
}

/// Returns true if the decoder description lists a 10-bit profile for the codec.
unsafe fn decoder_has_10bit_profile(dec: &mfxDecoderDescription, codec_id: u32) -> bool {
    if dec.NumCodecs == 0 || dec.Codecs.is_null() {
        return false;
    }
    for i in 0..dec.NumCodecs as usize {
        let entry = unsafe { &*dec.Codecs.add(i) };
        if entry.CodecID != codec_id {
            continue;
        }
        if entry.NumProfiles == 0 || entry.Profiles.is_null() {
            return false;
        }
        for j in 0..entry.NumProfiles as usize {
            let profile = unsafe { &*entry.Profiles.add(j) };
            if is_10bit_profile(codec_id, profile.Profile) {
                return true;
            }
        }
        break;
    }
    false
}

/// Collect all encoder profile IDs for the given codec.
unsafe fn encoder_profiles(enc: &mfxEncoderDescription, codec_id: u32) -> Vec<u32> {
    let mut profiles = Vec::new();
    if enc.NumCodecs == 0 || enc.Codecs.is_null() {
        return profiles;
    }
    for i in 0..enc.NumCodecs as usize {
        let entry = unsafe { &*enc.Codecs.add(i) };
        if entry.CodecID != codec_id {
            continue;
        }
        if !entry.Profiles.is_null() {
            for j in 0..entry.NumProfiles as usize {
                let profile = unsafe { &*entry.Profiles.add(j) };
                profiles.push(profile.Profile);
            }
        }
        break;
    }
    profiles
}

/// Returns true if any profile in the list indicates 10-bit encoding support.
fn encoder_supports_10bit(profiles: &[u32], codec_id: u32) -> bool {
    profiles.iter().any(|&p| is_10bit_profile(codec_id, p))
}

/// Returns true if the given profile ID implies 10-bit support for this codec.
fn is_10bit_profile(codec_id: u32, profile: u32) -> bool {
    match codec_id {
        id if id == MFX_CODEC_AVC => profile == MFX_PROFILE_AVC_HIGH10,
        id if id == MFX_CODEC_HEVC => profile == MFX_PROFILE_HEVC_MAIN10,
        id if id == MFX_CODEC_VP9 => matches!(profile, MFX_PROFILE_VP9_2 | MFX_PROFILE_VP9_3),
        // av1 main profile covers 8 and 10-bit
        id if id == MFX_CODEC_AV1 => true,
        _ => false,
    }
}

#[derive(Default)]
struct FilterTree {
    pixel_formats: HashSet<QsvFourCC>,
    /// every filter class lists all of its inputs x all of its outputs, so this is
    /// the union over filters
    convert_pairs: HashSet<(QsvFourCC, QsvFourCC)>,
    filters: HashSet<QsvFourCC>,
    rotation_formats: HashSet<QsvFourCC>,
    composite_pairs: HashSet<(QsvFourCC, QsvFourCC)>,
}

fn walk_filters(vpp: &mfxVPPDescription) -> FilterTree {
    let mut tree = FilterTree::default();
    if vpp.NumFilters == 0 || vpp.Filters.is_null() {
        return tree;
    }

    for i in 0..vpp.NumFilters as usize {
        let filter = unsafe { &*vpp.Filters.add(i) };
        tree.filters.insert(QsvFourCC(filter.FilterFourCC));
        if filter.NumMemTypes == 0 || filter.MemDesc.is_null() {
            continue;
        }

        for j in 0..filter.NumMemTypes as usize {
            let memdesc = unsafe { &*filter.MemDesc.add(j) };
            if memdesc.NumInFormats == 0 || memdesc.Formats.is_null() {
                continue;
            }
            for k in 0..memdesc.NumInFormats as usize {
                let fmt = unsafe { &*memdesc.Formats.add(k) };
                tree.pixel_formats.insert(QsvFourCC(fmt.InFormat));
                if fmt.NumOutFormat == 0 || fmt.OutFormats.is_null() {
                    continue;
                }

                for l in 0..fmt.NumOutFormat as usize {
                    let out = unsafe { &*fmt.OutFormats.add(l) };
                    tree.pixel_formats.insert(QsvFourCC(*out));
                    tree.convert_pairs
                        .insert((QsvFourCC(fmt.InFormat), QsvFourCC(*out)));
                    if filter.FilterFourCC == MFX_EXTBUFF_VPP_ROTATION && *out == fmt.InFormat {
                        tree.rotation_formats.insert(QsvFourCC(*out));
                    }
                    if filter.FilterFourCC == MFX_EXTBUFF_VPP_COMPOSITE {
                        tree.composite_pairs
                            .insert((QsvFourCC(fmt.InFormat), QsvFourCC(*out)));
                    }
                }
            }
        }
    }

    tree
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn va_filter_drops_decoders_the_driver_does_not_list() {
        use libva_sys::*;

        let mut decoders: HashMap<VideoFormat, Vec<u8>> = legacy::CODECS
            .iter()
            .map(|(format, _)| (*format, vec![8u8, 10u8]))
            .collect();
        // DG2 on iHD 26.3: no VC-1, VP8 or H.264 High 10
        let profiles: HashSet<VAProfile> = [
            VA_PROFILE_MPEG2_SIMPLE,
            VA_PROFILE_MPEG2_MAIN,
            VA_PROFILE_H264_MAIN,
            VA_PROFILE_H264_HIGH,
            VA_PROFILE_H264_CONSTRAINED_BASELINE,
            VA_PROFILE_HEVC_MAIN,
            VA_PROFILE_HEVC_MAIN10,
            VA_PROFILE_VP9_PROFILE0,
            VA_PROFILE_VP9_PROFILE2,
            VA_PROFILE_AV1_PROFILE0,
        ]
        .into();

        filter_decoders(&mut decoders, &profiles);

        let mut result: Vec<_> = decoders.into_iter().collect();
        result.sort_by_key(|(format, _)| format.to_string());
        let mut expected = vec![
            (VideoFormat::Av1, vec![8, 10]),
            (VideoFormat::H264, vec![8]),
            (VideoFormat::Hevc, vec![8, 10]),
            (VideoFormat::Mpeg2Video, vec![8]),
            (VideoFormat::Vp9, vec![8, 10]),
        ];
        expected.sort_by_key(|(format, _)| format.to_string());
        assert_eq!(result, expected);
    }

    #[test]
    fn rotation_requires_a_same_format_pair_in_the_rotation_filter() {
        let mut output = MFX_FOURCC_NV12;
        let mut format = mfxVPPDescription_filter_memdesc_format {
            InFormat: MFX_FOURCC_NV12,
            reserved: [0; 5],
            NumOutFormat: 1,
            OutFormats: &mut output,
        };
        // Zero initialization is valid because these structs contain only integers and raw pointers.
        let mut mem: mfxVPPDescription_filter_memdesc = unsafe { std::mem::zeroed() };
        mem.NumInFormats = 1;
        mem.Formats = &mut format;
        let mut filter: mfxVPPDescription_filter = unsafe { std::mem::zeroed() };
        filter.NumMemTypes = 1;
        filter.MemDesc = &mut mem;
        let mut vpp: mfxVPPDescription = unsafe { std::mem::zeroed() };
        vpp.NumFilters = 1;
        vpp.Filters = &mut filter;

        for filter_id in [MFX_EXTBUFF_VPP_ROTATION, MFX_EXTBUFF_VIDEO_SIGNAL_INFO_IN] {
            filter.FilterFourCC = filter_id;
            for input in [MFX_FOURCC_NV12, MFX_FOURCC_P010] {
                format.InFormat = input;
                mem.Formats = &mut format;
                filter.MemDesc = &mut mem;
                vpp.Filters = &mut filter;
                let tree = walk_filters(&vpp);
                assert!(tree.pixel_formats.contains(&QsvFourCC(input)));
                assert_eq!(
                    tree.rotation_formats.contains(&QsvFourCC(MFX_FOURCC_NV12)),
                    filter_id == MFX_EXTBUFF_VPP_ROTATION && input == MFX_FOURCC_NV12,
                );
                assert!(!tree.rotation_formats.contains(&QsvFourCC(MFX_FOURCC_P010)));
            }
        }
    }

    #[test]
    fn convert_pairs_keep_their_direction() {
        let mut output = MFX_FOURCC_NV12;
        let mut format = mfxVPPDescription_filter_memdesc_format {
            InFormat: MFX_FOURCC_P010,
            reserved: [0; 5],
            NumOutFormat: 1,
            OutFormats: &mut output,
        };
        // Zero initialization is valid because these structs contain only integers and raw pointers.
        let mut mem: mfxVPPDescription_filter_memdesc = unsafe { std::mem::zeroed() };
        mem.NumInFormats = 1;
        mem.Formats = &mut format;
        let mut filter: mfxVPPDescription_filter = unsafe { std::mem::zeroed() };
        filter.FilterFourCC = MFX_EXTBUFF_VPP_ROTATION;
        filter.NumMemTypes = 1;
        filter.MemDesc = &mut mem;
        let mut vpp: mfxVPPDescription = unsafe { std::mem::zeroed() };
        vpp.NumFilters = 1;
        vpp.Filters = &mut filter;

        let tree = walk_filters(&vpp);
        let nv12 = QsvFourCC(MFX_FOURCC_NV12);
        let p010 = QsvFourCC(MFX_FOURCC_P010);
        assert!(tree.convert_pairs.contains(&(p010, nv12)));
        assert!(!tree.convert_pairs.contains(&(nv12, p010)));
        assert!(!tree.convert_pairs.contains(&(p010, p010)));
    }

    #[test]
    fn composite_pairs_come_only_from_the_composite_filter() {
        let mut output = MFX_FOURCC_NV12;
        let mut format = mfxVPPDescription_filter_memdesc_format {
            InFormat: MFX_FOURCC_P010,
            reserved: [0; 5],
            NumOutFormat: 1,
            OutFormats: &mut output,
        };
        // Zero initialization is valid because these structs contain only integers and raw pointers.
        let mut mem: mfxVPPDescription_filter_memdesc = unsafe { std::mem::zeroed() };
        mem.NumInFormats = 1;
        mem.Formats = &mut format;
        let mut filter: mfxVPPDescription_filter = unsafe { std::mem::zeroed() };
        filter.NumMemTypes = 1;
        filter.MemDesc = &mut mem;
        let mut vpp: mfxVPPDescription = unsafe { std::mem::zeroed() };
        vpp.NumFilters = 1;
        vpp.Filters = &mut filter;

        for filter_id in [MFX_EXTBUFF_VPP_COMPOSITE, MFX_EXTBUFF_VPP_ROTATION] {
            filter.FilterFourCC = filter_id;
            vpp.Filters = &mut filter;
            let tree = walk_filters(&vpp);
            assert_eq!(
                tree.composite_pairs
                    .contains(&(QsvFourCC(MFX_FOURCC_P010), QsvFourCC(MFX_FOURCC_NV12))),
                filter_id == MFX_EXTBUFF_VPP_COMPOSITE,
            );
            assert!(
                !tree
                    .composite_pairs
                    .contains(&(QsvFourCC(MFX_FOURCC_NV12), QsvFourCC(MFX_FOURCC_P010)))
            );
        }
    }
}
