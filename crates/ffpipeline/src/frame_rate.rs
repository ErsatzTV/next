use serde::Serialize;

pub const MAX_TARGET_FRAME_RATE: u32 = 240;

#[derive(Debug, Clone, Serialize)]
pub struct FrameRate {
    pub r_frame_rate: String,
    pub parsed_frame_rate: f64,
}

impl FrameRate {
    pub fn parse(r_frame_rate: &str) -> FrameRate {
        let mut frame_rate = 24.0f64;

        if let Ok(parsed_frame_rate) = r_frame_rate.parse::<f64>() {
            frame_rate = parsed_frame_rate
        } else {
            let split: Vec<&str> = r_frame_rate.split('/').collect();
            if let Ok(left) = split[0].parse::<u32>()
                && let Ok(right) = split[1].parse::<u32>()
                && right != 0
            {
                frame_rate = (left as f64) / (right as f64);
            }
        }

        FrameRate {
            r_frame_rate: r_frame_rate.to_owned(),
            parsed_frame_rate: frame_rate,
        }
    }

    /// Rejects decimals: 29.97 is not 30000/1001.
    pub fn parse_target(value: &str) -> Option<FrameRate> {
        let (num, den) = parse_rational(value)?;
        if num == 0 || den == 0 || num < den || num > den * u64::from(MAX_TARGET_FRAME_RATE) {
            return None;
        }

        Some(FrameRate {
            r_frame_rate: value.to_owned(),
            parsed_frame_rate: num as f64 / den as f64,
        })
    }

    /// `0/0` never matches, though `parse` reads it as 24.
    pub fn same_rate(&self, other: &FrameRate) -> bool {
        match (
            parse_rational(&self.r_frame_rate),
            parse_rational(&other.r_frame_rate),
        ) {
            (Some((n1, d1)), Some((n2, d2))) => n1 != 0 && d1 != 0 && d2 != 0 && n1 * d2 == n2 * d1,
            _ => false,
        }
    }
}

fn parse_rational(value: &str) -> Option<(u64, u64)> {
    let digits = |s: &str| {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u32>().ok().map(u64::from))
            .flatten()
    };

    match value.split_once('/') {
        Some((num, den)) => Some((digits(num)?, digits(den)?)),
        None => Some((digits(value)?, 1)),
    }
}

impl Default for FrameRate {
    fn default() -> Self {
        FrameRate {
            r_frame_rate: String::from("24"),
            parsed_frame_rate: 24.0f64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_accepts_integer_and_rational_rates() {
        for (value, expected) in [
            ("25", 25.0),
            ("30000/1001", 30000.0 / 1001.0),
            ("60/1", 60.0),
            ("1", 1.0),
            ("240", 240.0),
        ] {
            let rate = FrameRate::parse_target(value).unwrap();
            assert_eq!(rate.r_frame_rate, value);
            assert_eq!(rate.parsed_frame_rate, expected);
        }
    }

    #[test]
    fn parse_target_rejects_decimals_and_out_of_range_rates() {
        for value in [
            "",
            "29.97",
            "0",
            "30/0",
            "0/1",
            "abc",
            "+30",
            "-30",
            " 30",
            "30/",
            "/1",
            "1/2",
            "241",
            "30/1/1",
            "4294967296",
        ] {
            assert!(FrameRate::parse_target(value).is_none(), "{value}");
        }
    }

    #[test]
    fn same_rate_compares_rationals() {
        let ntsc = FrameRate::parse_target("30000/1001").unwrap();
        assert!(FrameRate::parse("30000/1001").same_rate(&ntsc));
        assert!(FrameRate::parse("60000/2002").same_rate(&ntsc));
        assert!(!FrameRate::parse("30/1").same_rate(&ntsc));
        assert!(FrameRate::parse("25/1").same_rate(&FrameRate::parse_target("25").unwrap()));
        assert!(!FrameRate::parse("0/0").same_rate(&FrameRate::parse_target("24").unwrap()));
        assert!(
            !FrameRate::parse("23.976").same_rate(&FrameRate::parse_target("24000/1001").unwrap())
        );
    }
}
