use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::convert::Into;
use std::iter::Iterator;
use std::path::Path;
use std::sync::LazyLock;

use strum::{Display, EnumIter, IntoEnumIterator, IntoStaticStr};
use tokio::process::Command;

use crate::error::FFPipelineError;

static KNOWN_ACCELS: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| KnownHardwareAccel::iter().map(|x| x.into()).collect());

static KNOWN_FILTERS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    KnownVideoFilter::iter()
        .map(|x| x.into())
        .collect::<Vec<&str>>()
});

#[derive(Display, EnumIter, IntoStaticStr, Debug, PartialEq)]
pub enum KnownHardwareAccel {
    #[strum(serialize = "cuda")]
    Cuda,
    #[strum(serialize = "qsv")]
    Qsv,
    #[strum(serialize = "vaapi")]
    Vaapi,
    #[strum(serialize = "videotoolbox")]
    VideoToolbox,
    #[strum(serialize = "vulkan")]
    Vulkan,
    #[strum(serialize = "opencl")]
    Opencl,
}

/// Convert a KnownHardwareAccel to a Cow-wrapped &'static str.
impl From<KnownHardwareAccel> for Cow<'static, str> {
    fn from(value: KnownHardwareAccel) -> Self {
        Cow::<'static, str>::from(<KnownHardwareAccel as Into<&'static str>>::into(value))
    }
}

#[derive(Display, EnumIter, IntoStaticStr, Debug, PartialEq)]
pub enum KnownVideoFilter {
    #[strum(serialize = "bwdif")]
    Bwdif,
    #[strum(serialize = "bwdif_cuda")]
    BwdifCuda,
    #[strum(serialize = "deinterlace_qsv")]
    DeinterlaceQsv,
    #[strum(serialize = "libplacebo")]
    LibPlacebo,
    #[strum(serialize = "overlay_cuda")]
    OverlayCuda,
    #[strum(serialize = "pad_cuda")]
    PadCuda,
    #[strum(serialize = "pad_vaapi")]
    PadVaapi,
    #[strum(serialize = "scale_cuda")]
    ScaleCuda,
    #[strum(serialize = "scale_vaapi")]
    ScaleVaapi,
    #[strum(serialize = "scale_vt")]
    ScaleVt,
    #[strum(serialize = "scale_vulkan")]
    ScaleVulkan,
    #[strum(serialize = "tonemap_opencl")]
    TonemapOpencl,
    #[strum(serialize = "tonemap_vaapi")]
    TonemapVaapi,
    #[strum(serialize = "vpp_qsv")]
    VppQsv,
    #[strum(serialize = "w3fdif")]
    W3fdif,
    #[strum(serialize = "yadif")]
    Yadif,
    #[strum(serialize = "yadif_cuda")]
    YadifCuda,
}

#[derive(Debug, Clone, Default)]
pub struct FfmpegInfo {
    pub(crate) hwaccels: HashSet<String>,
    pub(crate) video_filters: HashSet<String>,
    pub(crate) preferred_filters: HashMap<String, usize>,
}

impl FfmpegInfo {
    pub async fn load(
        path: &Path,
        disabled_filters: &[String],
        preferred_filters: &[String],
    ) -> Result<FfmpegInfo, FFPipelineError> {
        let hwaccels = Self::load_hw_accels(path).await?;
        let video_filters = Self::load_video_filters(path, disabled_filters).await?;

        // filter preferred by known video filters
        let mut preferred: HashMap<String, usize> = HashMap::new();
        for (idx, filter) in preferred_filters.iter().enumerate() {
            if video_filters.contains(filter) {
                preferred.insert(filter.clone(), idx);
            }
        }

        Ok(FfmpegInfo {
            hwaccels,
            video_filters,
            preferred_filters: preferred,
        })
    }

    pub fn has_hw_accel(&self, hw_accel: &KnownHardwareAccel) -> bool {
        let accel_string = hw_accel.to_string();
        self.hwaccels.iter().any(|f| f == &accel_string)
    }

    pub fn has_video_filter(&self, filter: &KnownVideoFilter) -> bool {
        self.video_filters.contains(&filter.to_string())
    }

    /// Returns the "best" known filter from the inputted set. "Best" in this case is defined
    /// as 1. is a filter that the queried ffmpeg contains and 2. has the lowest preference index
    /// (i.e. index-0 in the preference list has higher priority than index-1)
    /// NOTE: If NONE of the inputted filters exist in the preference list, the _first_ entry
    /// in the inputted list will be returned.
    pub fn find_best_fit<'a>(
        &self,
        filter_options: &'a [KnownVideoFilter],
    ) -> Option<&'a KnownVideoFilter> {
        filter_options
            .iter()
            .filter(|f| self.has_video_filter(f))
            .min_by_key(|f| self.preference_position(f))
    }

    /// Returns the preference index for the video filter. If the filter is not known, or does not
    /// exist in the preference list, returns `usize::MAX`.
    fn preference_position(&self, filter: &KnownVideoFilter) -> usize {
        let filter_string = filter.to_string();
        self.preferred_filters
            .get(&filter_string)
            .copied()
            .unwrap_or(usize::MAX)
    }

    async fn load_hw_accels(path: &Path) -> Result<HashSet<String>, FFPipelineError> {
        let output = Command::new(path)
            .args(["-hide_banner", "-hwaccels"])
            .output()
            .await
            .map_err(|_| FFPipelineError::FfmpegCapabilitiesError(String::from("hwaccels")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut accels: HashSet<String> = HashSet::new();

        for line in stdout.lines() {
            let trimmed = line.trim();

            if trimmed.contains(":") || trimmed.is_empty() {
                continue;
            }

            if KNOWN_ACCELS.contains(&trimmed) {
                accels.insert(trimmed.to_owned());
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

        let mut filters: HashSet<String> = HashSet::new();

        for line in stdout.lines() {
            //  .. scale_cuda        V->V       GPU accelerated video resizer
            if let Some(filter) = line.split_whitespace().nth(1)
                && KNOWN_FILTERS.contains(&filter)
                && !disabled_filters.iter().any(|f| f == filter)
            {
                filters.insert(filter.to_owned());
            }
        }

        Ok(filters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_best_fit_no_preference_select_first() {
        let mut video_filters = HashSet::new();
        video_filters.extend(
            [
                KnownVideoFilter::TonemapOpencl,
                KnownVideoFilter::TonemapVaapi,
            ]
            .iter()
            .map(ToString::to_string),
        );

        let info: FfmpegInfo = FfmpegInfo {
            hwaccels: HashSet::new(),
            video_filters,
            preferred_filters: HashMap::new(),
        };

        let best_fit = info.find_best_fit(
            [
                KnownVideoFilter::TonemapOpencl,
                KnownVideoFilter::TonemapVaapi,
            ]
            .as_ref(),
        );

        assert!(info.has_video_filter(&KnownVideoFilter::TonemapOpencl));
        assert_eq!(best_fit, Some(&KnownVideoFilter::TonemapOpencl));
    }

    #[test]
    fn test_best_fit_preference_select_by_preference() {
        let mut video_filters = HashSet::new();
        video_filters.extend(
            [
                KnownVideoFilter::TonemapOpencl,
                KnownVideoFilter::TonemapVaapi,
            ]
            .iter()
            .map(ToString::to_string),
        );

        let mut preferred_filters = HashMap::new();
        preferred_filters.insert(KnownVideoFilter::TonemapOpencl.to_string(), 1);
        preferred_filters.insert(KnownVideoFilter::TonemapVaapi.to_string(), 0);

        let info = FfmpegInfo {
            hwaccels: HashSet::new(),
            video_filters,
            preferred_filters,
        };

        let best_fit = info.find_best_fit(
            [
                KnownVideoFilter::TonemapOpencl,
                KnownVideoFilter::TonemapVaapi,
            ]
            .as_ref(),
        );

        assert_eq!(best_fit, Some(&KnownVideoFilter::TonemapVaapi));
    }
}
