use serde::Serialize;

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
}

impl Default for FrameRate {
    fn default() -> Self {
        FrameRate {
            r_frame_rate: String::from("24"),
            parsed_frame_rate: 24.0f64,
        }
    }
}
