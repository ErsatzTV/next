use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::convert::Into;
use std::iter::Iterator;
use std::path::Path;
use std::sync::LazyLock;

use serde::Serialize;
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

/// Filters whose AVOptions are probed at load time, so pipeline builders can
/// gate on options that only exist in patched or newer ffmpeg builds (e.g.
/// vpp_qsv pad_w/pad_h).
static OPTION_PROBED_FILTERS: &[KnownVideoFilter] = &[KnownVideoFilter::VppQsv];

#[derive(Display, EnumIter, IntoStaticStr, Debug, PartialEq)]
pub enum KnownHardwareAccel {
    #[strum(serialize = "cuda")]
    Cuda,
    #[strum(serialize = "qsv")]
    Qsv,
    #[strum(serialize = "rkmpp")]
    Rkmpp,
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
    #[strum(serialize = "deinterlace_vaapi")]
    DeinterlaceVaapi,
    #[strum(serialize = "libplacebo")]
    LibPlacebo,
    #[strum(serialize = "overlay_cuda")]
    OverlayCuda,
    #[strum(serialize = "overlay_vaapi")]
    OverlayVaapi,
    #[strum(serialize = "pad_cuda")]
    PadCuda,
    #[strum(serialize = "pad_opencl")]
    PadOpencl,
    #[strum(serialize = "pad_vaapi")]
    PadVaapi,
    #[strum(serialize = "scale_cuda")]
    ScaleCuda,
    #[strum(serialize = "scale_rkrga")]
    ScaleRkrga,
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

#[derive(Debug, Clone, Default, Serialize)]
pub struct FfmpegInfo {
    pub(crate) hwaccels: HashSet<String>,
    pub(crate) video_filters: HashSet<String>,
    pub(crate) preferred_filters: HashMap<String, usize>,
    pub(crate) video_filter_options: HashMap<String, HashSet<String>>,
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

        let mut video_filter_options: HashMap<String, HashSet<String>> = HashMap::new();
        for filter in OPTION_PROBED_FILTERS {
            let name = filter.to_string();
            if video_filters.contains(&name) {
                let options = Self::load_video_filter_options(path, &name).await?;
                video_filter_options.insert(name, options);
            }
        }

        Ok(FfmpegInfo {
            hwaccels,
            video_filters,
            preferred_filters: preferred,
            video_filter_options,
        })
    }

    pub fn has_hw_accel(&self, hw_accel: &KnownHardwareAccel) -> bool {
        let accel_string = hw_accel.to_string();
        self.hwaccels.iter().any(|f| f == &accel_string)
    }

    pub fn has_video_filter(&self, filter: &KnownVideoFilter) -> bool {
        self.video_filters.contains(&filter.to_string())
    }

    /// Returns true when the filter is present AND advertises the named
    /// AVOption. Only filters in [`OPTION_PROBED_FILTERS`] have their options
    /// probed; all other filters always return false.
    pub fn video_filter_has_option(&self, filter: &KnownVideoFilter, option: &str) -> bool {
        self.video_filter_options
            .get(&filter.to_string())
            .is_some_and(|options| options.contains(option))
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

    pub fn escape_filter_value(value: &str) -> String {
        let mut out = String::with_capacity(value.len() + 8);
        for ch in value.chars() {
            match ch {
                // filtergraph delimeters get one backslash
                '[' | ']' | ',' | ';' => {
                    out.push('\\');
                    out.push(ch);
                }
                // colon only needs two backslashes
                ':' => {
                    out.push_str("\\\\");
                    out.push(ch);
                }
                // apostrophe needs three backslashes
                '\'' => {
                    out.push_str("\\\\\\");
                    out.push(ch);
                }
                // literal backslash needs to be four
                '\\' => out.push_str("\\\\\\\\"),
                _ => out.push(ch),
            }
        }

        out
    }

    pub fn escape_path(path: &str) -> String {
        #[cfg(target_os = "windows")]
        let path = &path.replace('\\', "/");

        Self::escape_filter_value(path)
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

    async fn load_video_filter_options(
        path: &Path,
        filter: &str,
    ) -> Result<HashSet<String>, FFPipelineError> {
        let output = Command::new(path)
            .args(["-hide_banner", "-h", &format!("filter={filter}")])
            .output()
            .await
            .map_err(|_| {
                FFPipelineError::FfmpegCapabilitiesError(format!("filter options: {filter}"))
            })?;

        Ok(Self::parse_filter_options(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }

    /// Parses `ffmpeg -h filter=NAME` output into the set of AVOption names.
    /// Option lines look like `   pad_w   <int>   ..FV....... description`;
    /// the `<type>` in the second column is what distinguishes them from the
    /// surrounding header lines.
    fn parse_filter_options(help_text: &str) -> HashSet<String> {
        let mut options = HashSet::new();

        for line in help_text.lines() {
            let mut parts = line.split_whitespace();
            if let (Some(name), Some(kind)) = (parts.next(), parts.next())
                && kind.starts_with('<')
            {
                options.insert(name.to_owned());
            }
        }

        options
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
            video_filter_options: HashMap::new(),
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
            video_filter_options: HashMap::new(),
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

    #[test]
    fn test_parse_filter_options() {
        let help_text = r"Filter vpp_qsv
  Description: Quick Sync Video VPP.
  Inputs:
       #0: default (video)
  Outputs:
       #0: default (video)
vpp_qsv AVOptions:
   deinterlace       <int>        ..FV....... deinterlace mode: 0=off, 1=bob, 2=advanced (from 0 to 2) (default 0)
   denoise           <int>        ..FV....... denoise level [0, 100] (from 0 to 100) (default 0)
   pad_w             <int>        ..FV....... set the padded output width (0 = no padding) (from 0 to 32767) (default 0)
   pad_h             <int>        ..FV....... set the padded output height (0 = no padding) (from 0 to 32767) (default 0)
   pad_color         <color>      ..FV....... set the colour of the padded area (default 'black')
";
        let options = FfmpegInfo::parse_filter_options(help_text);

        assert!(options.contains("deinterlace"));
        assert!(options.contains("pad_w"));
        assert!(options.contains("pad_color"));
        assert!(!options.contains("#0:"));
        assert!(!options.contains("Description:"));
    }

    #[test]
    fn test_video_filter_has_option() {
        let mut video_filters = HashSet::new();
        video_filters.insert(KnownVideoFilter::VppQsv.to_string());

        let mut video_filter_options = HashMap::new();
        video_filter_options.insert(
            KnownVideoFilter::VppQsv.to_string(),
            HashSet::from([String::from("pad_w"), String::from("pad_h")]),
        );

        let info = FfmpegInfo {
            video_filters,
            video_filter_options,
            ..Default::default()
        };

        assert!(info.video_filter_has_option(&KnownVideoFilter::VppQsv, "pad_w"));
        assert!(!info.video_filter_has_option(&KnownVideoFilter::VppQsv, "does_not_exist"));
        assert!(!info.video_filter_has_option(&KnownVideoFilter::PadVaapi, "pad_w"));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_windows_escape_path() {
        let input = r"C:\Movies\foo[1].srt";
        let result = FfmpegInfo::escape_path(input);
        assert_eq!(result, r"C\\:/Movies/foo\[1\].srt");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_linux_escape_path() {
        let input = r"/movies/Big Buck Bunny [2008]/Big Buck Bunny [2008].en.srt";
        let result = FfmpegInfo::escape_path(input);
        assert_eq!(
            result,
            r"/movies/Big Buck Bunny \[2008\]/Big Buck Bunny \[2008\].en.srt"
        );
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_linux_escape_path_apostrophe() {
        let input = r"/media/World's [2019]/ep's.mkv";
        let result = FfmpegInfo::escape_path(input);
        assert_eq!(result, r"/media/World\\\'s \[2019\]/ep\\\'s.mkv");
    }
}
