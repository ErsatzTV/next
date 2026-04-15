use libloading::Library;

pub const CUDA_VIDEO_CODEC_MPEG2: u32 = 1;
pub const CUDA_VIDEO_CODEC_VC1: u32 = 3;
pub const CUDA_VIDEO_CODEC_H264: u32 = 4;
pub const CUDA_VIDEO_CODEC_HEVC: u32 = 8;
pub const CUDA_VIDEO_CODEC_VP8: u32 = 9;
pub const CUDA_VIDEO_CODEC_VP9: u32 = 10;
pub const CUDA_VIDEO_CODEC_AV1: u32 = 13;

pub const CUDA_VIDEO_CHROMA_FORMAT_420: u32 = 1;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct CuvidDecodeCaps {
    pub e_codec_type: u32,       // IN
    pub e_chroma_format: u32,    // IN
    pub n_bit_depth_minus8: u32, // IN: 0 = 8-bit, 2 = 10-bit
    pub reserved1: [u32; 3],
    pub b_is_supported: u32, // OUT: 1 if supported
    pub n_num_nvdecs: u8,
    pub _pad: u8,
    pub n_output_format_mask: u16,
    pub n_max_width: u32,
    pub n_max_height: u32,
    pub n_max_mb_count: u32,
    pub n_min_width: u16,
    pub n_min_height: u16,
    pub b_is_histogram_supported: u8,
    pub n_counter_bit_depth: u8,
    pub n_max_histogram_bins: u16,
    pub reserved3: [u32; 10],
}

const _: () = assert!(size_of::<CuvidDecodeCaps>() == 92);

pub struct CuvidLib {
    _lib: Library,
    pub cuvid_get_decoder_caps: unsafe extern "C" fn(*mut CuvidDecodeCaps) -> i32,
}

impl CuvidLib {
    pub fn load() -> Result<Self, libloading::Error> {
        #[cfg(target_os = "linux")]
        let name = "libnvcuvid.so.1";
        #[cfg(target_os = "windows")]
        let name = "nvcuvid.dll";
        unsafe {
            let lib = Library::new(name)?;
            let cuvid_get_decoder_caps = *lib.get(b"cuvidGetDecoderCaps\0")?;
            Ok(Self {
                _lib: lib,
                cuvid_get_decoder_caps,
            })
        }
    }
}
