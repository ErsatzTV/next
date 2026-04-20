use std::collections::HashSet;
use std::path::Path;

use strum::{Display, EnumIter, IntoEnumIterator};
use tokio::process::Command;

use crate::error::FFPipelineError;

#[derive(Display, EnumIter, PartialEq)]
#[strum(serialize_all = "snake_case")]
pub enum KnownHardwareAccel {
    Cuda,
    Qsv,
    Vaapi,
    #[strum(serialize = "videotoolbox")]
    VideoToolbox,
    Vulkan,
}

#[derive(Display, EnumIter, PartialEq)]
#[strum(serialize_all = "snake_case")]
pub enum KnownVideoFilter {
    Bwdif,
    #[strum(serialize = "libplacebo")]
    LibPlacebo,
    PadCuda,
    PadVaapi,
    ScaleCuda,
    ScaleVaapi,
    ScaleVulkan,
    VppQsv,
    W3fdif,
    Yadif,
}

#[derive(Debug, Clone, Default)]
pub struct FfmpegInfo {
    hwaccels: Vec<String>,
    video_filters: HashSet<String>,
    preferred_filters: Vec<String>,
}

impl FfmpegInfo {
    pub async fn load(
        path: &Path,
        disabled_filters: &[String],
        preferred_filters: &[String],
    ) -> Result<FfmpegInfo, FFPipelineError> {
        let hwaccels = Self::load_hw_accels(path).await?;
        let video_filters = Self::load_video_filters(path, disabled_filters).await?;

        // filter preferred by video_filters
        let mut preferred: Vec<String> = Vec::new();
        for filter in preferred_filters {
            if video_filters.contains(filter) {
                preferred.push(filter.clone());
            }
        }

        Ok(FfmpegInfo {
            hwaccels,
            video_filters,
            preferred_filters: preferred,
        })
    }

    pub fn has_hw_accel(&self, hw_accel: &KnownHardwareAccel) -> bool {
        self.hwaccels.contains(&hw_accel.to_string())
    }

    pub fn has_video_filter(&self, filter: &KnownVideoFilter) -> bool {
        self.video_filters.contains(&filter.to_string())
    }

    pub fn is_preferred_filter(&self, filter: &KnownVideoFilter) -> bool {
        self.preferred_filters.contains(&filter.to_string())
    }

    async fn load_hw_accels(path: &Path) -> Result<Vec<String>, FFPipelineError> {
        let output = Command::new(path)
            .args(["-hide_banner", "-hwaccels"])
            .output()
            .await
            .map_err(|_| FFPipelineError::FfmpegCapabilitiesError(String::from("hwaccels")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        let known_accels: HashSet<String> =
            KnownHardwareAccel::iter().map(|f| f.to_string()).collect();

        let mut accels: Vec<String> = Vec::new();

        for line in stdout.lines() {
            let trimmed = line.trim();

            if trimmed.contains(":") || trimmed.is_empty() {
                continue;
            }

            if known_accels.contains(trimmed) {
                accels.push(trimmed.to_owned());
            }
        }

        Ok(accels)
    }

    async fn load_video_filters(
        path: &Path,
        disabled_filters: &[String],
    ) -> Result<HashSet<String>, FFPipelineError> {
        let output = Command::new(path)
            .args(["-hide_banner", "-filters"])
            .output()
            .await
            .map_err(|_| FFPipelineError::FfmpegCapabilitiesError(String::from("filters")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        let known_filters: HashSet<String> =
            KnownVideoFilter::iter().map(|f| f.to_string()).collect();

        let mut filters: HashSet<String> = HashSet::new();

        for line in stdout.lines() {
            //  .. scale_cuda        V->V       GPU accelerated video resizer
            if let Some(filter) = line.split_whitespace().nth(1)
                && known_filters.contains(filter)
                && !disabled_filters.iter().any(|f| f == filter)
            {
                filters.insert(filter.to_owned());
            }
        }

        Ok(filters)
    }
}
