use std::path::Path;

use tokio::process::Command;

use crate::error::FFPipelineError;

static KNOWN_ACCELS: &[&str] = &["cuda", "qsv", "vaapi", "videotoolbox", "vulkan"];
static KNOWN_FILTERS: &[&str] = &[
    "libplacebo",
    "pad_cuda",
    "pad_vaapi",
    "scale_cuda",
    "scale_vaapi",
    "vpp_qsv",
];

pub enum KnownHardwareAccel {
    Cuda,
    Qsv,
    Vaapi,
    VideoToolbox,
    Vulkan,
}

pub enum KnownVideoFilter {
    LibPlacebo,
    PadCuda,
    PadVaapi,
    ScaleCuda,
    ScaleVaapi,
    VppQsv,
}

#[derive(Debug, Clone, Default)]
pub struct FfmpegInfo {
    hwaccels: Vec<String>,
    video_filters: Vec<String>,
}

impl FfmpegInfo {
    pub async fn load(
        path: &Path,
        disabled_filters: &[String],
    ) -> Result<FfmpegInfo, FFPipelineError> {
        let hwaccels = Self::load_hw_accels(path).await?;
        let video_filters = Self::load_video_filters(path, disabled_filters).await?;
        Ok(FfmpegInfo {
            hwaccels,
            video_filters,
        })
    }

    pub fn has_hw_accel(&self, hw_accel: &KnownHardwareAccel) -> bool {
        let accel_string = match hw_accel {
            KnownHardwareAccel::Cuda => "cuda",
            KnownHardwareAccel::Qsv => "qsv",
            KnownHardwareAccel::Vaapi => "vaapi",
            KnownHardwareAccel::VideoToolbox => "videotoolbox",
            KnownHardwareAccel::Vulkan => "vulkan",
        };

        self.hwaccels.iter().any(|f| f == accel_string)
    }

    pub fn has_video_filter(&self, filter: &KnownVideoFilter) -> bool {
        let filter_string = match filter {
            KnownVideoFilter::LibPlacebo => "libplacebo",
            KnownVideoFilter::PadCuda => "pad_cuda",
            KnownVideoFilter::PadVaapi => "pad_vaapi",
            KnownVideoFilter::ScaleCuda => "scale_cuda",
            KnownVideoFilter::ScaleVaapi => "scale_vaapi",
            KnownVideoFilter::VppQsv => "vpp_qsv",
        };

        self.video_filters.iter().any(|f| f == filter_string)
    }

    async fn load_hw_accels(path: &Path) -> Result<Vec<String>, FFPipelineError> {
        let output = Command::new(path)
            .args(["-hide_banner", "-hwaccels"])
            .output()
            .await
            .map_err(|_| FFPipelineError::FfmpegCapabilitiesError(String::from("hwaccels")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut accels: Vec<String> = Vec::new();

        for line in stdout.lines() {
            let trimmed = line.trim();

            if trimmed.contains(":") || trimmed.is_empty() {
                continue;
            }

            if KNOWN_ACCELS.contains(&trimmed) {
                accels.push(trimmed.to_owned());
            }
        }

        Ok(accels)
    }

    async fn load_video_filters(
        path: &Path,
        disabled_filters: &[String],
    ) -> Result<Vec<String>, FFPipelineError> {
        let output = Command::new(path)
            .args(["-hide_banner", "-filters"])
            .output()
            .await
            .map_err(|_| FFPipelineError::FfmpegCapabilitiesError(String::from("filters")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut filters: Vec<String> = Vec::new();

        for line in stdout.lines() {
            //  .. scale_cuda        V->V       GPU accelerated video resizer
            if let Some(filter) = line.split_whitespace().nth(1)
                && KNOWN_FILTERS.contains(&filter)
                && !disabled_filters.iter().any(|f| f == filter)
            {
                filters.push(filter.to_owned());
            }
        }

        Ok(filters)
    }
}
