use crate::probe::ProbeResultColorParams;

// from libavfilter/vf_setparams.c
static PRIMARIES: &[&str] = &[
    "bt709",
    "bt470m",
    "bt470bg",
    "smpte170m",
    "smpte240m",
    "film",
    "bt2020",
    "smpte428",
    "smpte431",
    "smpte432",
    "jedec-p22",
    "ebu3213",
    "vgamut",
];

static TRANSFERS: &[&str] = &[
    "bt709",
    "bt470m",
    "bt470bg",
    "smpte170m",
    "smpte240m",
    "linear",
    "log100",
    "log316",
    "iec61966-2-4",
    "bt1361e",
    "iec61966-2-1",
    "bt2020-10",
    "bt2020-12",
    "smpte2084",
    "smpte428",
    "arib-std-b67",
    "vlog",
];

static MATRICES: &[&str] = &[
    "gbr",
    "bt709",
    "fcc",
    "bt470bg",
    "smpte170m",
    "smpte240m",
    "ycgco",
    "ycgco-re",
    "ycgco-ro",
    "bt2020nc",
    "bt2020c",
    "smpte2085",
    "chroma-derived-nc",
    "chroma-derived-c",
    "ictcp",
    "ipt-c2",
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ColorTag {
    #[default]
    Unspecified,
    Named(&'static str),
    /// Not unspecified: a bad hint for an HDR transfer must not become SDR tags.
    Unrecognized,
}

impl ColorTag {
    fn parse(value: Option<&str>, accepted: &[&'static str]) -> Self {
        match value.map(str::trim) {
            None | Some("" | "unknown" | "unspecified" | "reserved") => Self::Unspecified,
            Some(value) => accepted
                .iter()
                .find(|name| **name == value)
                .map_or(Self::Unrecognized, |name| Self::Named(name)),
        }
    }

    fn setparams_value(self) -> Option<&'static str> {
        match self {
            Self::Unspecified => Some("unknown"),
            Self::Named(name) => Some(name),
            Self::Unrecognized => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameColor {
    primaries: ColorTag,
    transfer: ColorTag,
    matrix: ColorTag,
}

impl FrameColor {
    pub(crate) const fn bt709() -> Self {
        Self {
            primaries: ColorTag::Named("bt709"),
            transfer: ColorTag::Named("bt709"),
            matrix: ColorTag::Named("bt709"),
        }
    }

    pub(crate) const fn hdr10() -> Self {
        Self {
            primaries: ColorTag::Named("bt2020"),
            transfer: ColorTag::Named("smpte2084"),
            matrix: ColorTag::Named("bt2020nc"),
        }
    }

    pub(crate) fn is_bt2020(&self) -> bool {
        matches!(self.matrix, ColorTag::Named("bt2020nc" | "bt2020c"))
    }

    /// `None` for an unrecognized name: setparams would fail the filter graph.
    pub(crate) fn to_setparams(self) -> Option<String> {
        Some(format!(
            "setparams=color_primaries={}:color_trc={}:colorspace={}",
            self.primaries.setparams_value()?,
            self.transfer.setparams_value()?,
            self.matrix.setparams_value()?
        ))
    }

    pub(crate) fn as_bt709_around(self, filter: &str) -> Option<String> {
        let restore = self.to_setparams()?;
        let bt709 = Self::bt709().to_setparams()?;
        Some(format!("{bt709},{filter},{restore}"))
    }
}

impl From<&ProbeResultColorParams> for FrameColor {
    fn from(params: &ProbeResultColorParams) -> Self {
        Self {
            primaries: ColorTag::parse(params.color_primaries.as_deref(), PRIMARIES),
            transfer: ColorTag::parse(params.color_transfer.as_deref(), TRANSFERS),
            matrix: ColorTag::parse(params.color_space.as_deref(), MATRICES),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn frame_color(
        primaries: Option<&str>,
        transfer: Option<&str>,
        space: Option<&str>,
    ) -> FrameColor {
        FrameColor::from(&ProbeResultColorParams {
            color_primaries: primaries.map(String::from),
            color_transfer: transfer.map(String::from),
            color_space: space.map(String::from),
            ..Default::default()
        })
    }

    #[rstest]
    #[case::hdr10(
        Some("bt2020"),
        Some("smpte2084"),
        Some("bt2020nc"),
        Some("setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc")
    )]
    #[case::missing_and_empty(
        None,
        Some(""),
        Some("bt2020nc"),
        Some("setparams=color_primaries=unknown:color_trc=unknown:colorspace=bt2020nc")
    )]
    #[case::reserved(
        Some("reserved"),
        Some("bt2020-10"),
        Some("bt2020c"),
        Some("setparams=color_primaries=unknown:color_trc=bt2020-10:colorspace=bt2020c")
    )]
    #[case::unrecognized(Some("bt2020"), Some("pq"), Some("bt2020nc"), None)]
    fn to_setparams(
        #[case] primaries: Option<&str>,
        #[case] transfer: Option<&str>,
        #[case] space: Option<&str>,
        #[case] expected: Option<&str>,
    ) {
        assert_eq!(
            frame_color(primaries, transfer, space)
                .to_setparams()
                .as_deref(),
            expected
        );
    }

    #[rstest]
    #[case::ncl(Some("bt2020nc"), true)]
    #[case::cl(Some("bt2020c"), true)]
    #[case::bt709(Some("bt709"), false)]
    #[case::unspecified(None, false)]
    fn is_bt2020(#[case] space: Option<&str>, #[case] expected: bool) {
        assert_eq!(frame_color(None, None, space).is_bt2020(), expected);
    }

    #[test]
    fn as_bt709_around_restores_source_tags() {
        assert_eq!(
            FrameColor::hdr10().as_bt709_around("pad_vaapi").as_deref(),
            Some(
                "setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709,pad_vaapi,\
                 setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc"
            )
        );
    }
}
