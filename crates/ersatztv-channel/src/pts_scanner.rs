use std::path::PathBuf;
use std::time::Duration;

use ersatztv_channel::config::ChannelConfig;
use ersatztv_channel::error::ChannelError;
use tokio::process::Command;

pub struct PtsTime {
    pub duration: Duration,
}

pub struct PtsScanner {
    output_folder: PathBuf,
}

impl PtsScanner {
    pub fn new(channel_config: &ChannelConfig) -> PtsScanner {
        PtsScanner {
            output_folder: channel_config.expanded_output_folder().to_owned(),
        }
    }

    pub async fn get_last_pts(&self) -> Result<PtsTime, ChannelError> {
        let mut pts_time = PtsTime {
            duration: Duration::ZERO,
        };

        // find last segment file in output folder
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&self.output_folder).await?;
        while let Ok(Some(entry)) = dir.next_entry().await {
            if entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
            {
                entries.push(entry);
            }
        }
        entries.sort_by_key(|a| std::cmp::Reverse(a.file_name()));
        if let Some(last_segment) = entries.first() {
            // call ffprobe
            let path = last_segment
                .path()
                .into_os_string()
                .into_string()
                .map_err(|_| ChannelError::PtsScannerFailure)?;

            let output = Command::new("ffprobe")
                .args([
                    "-v",
                    "-0",
                    "-show_entries",
                    "packet=pts_time,duration_time",
                    "-of",
                    "compact=p=0:nk=1",
                    &path,
                ])
                .output()
                .await
                .map_err(|_| ChannelError::PtsScannerFailure)?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            pts_time.duration = largest_pts(&stdout);
        }

        Ok(pts_time)
    }
}

/// The largest `pts_time` plus `duration_time` across ffprobe's packet
/// output, or zero when no line carries one.
///
/// A packet is skipped rather than trusted whenever its fields are missing
/// or unparseable, and whenever the total will not fit a [`Duration`].
/// ffprobe reports a negative `pts_time` for the packets that lead a
/// timestamp discontinuity, and reports `N/A` before a stream's first
/// timestamp is known. Neither is ever a segment's largest pts, so dropping
/// them costs nothing, where converting a negative one panics and takes the
/// whole channel down with it.
fn largest_pts(probe_output: &str) -> Duration {
    let mut largest = Duration::ZERO;

    for line in probe_output.lines() {
        let mut fields = line.trim().split('|');

        let Some(Ok(seconds)) = fields.next().map(str::parse::<f64>) else {
            continue;
        };

        let mut total_seconds = seconds;
        if let Some(Ok(seconds)) = fields.next().map(str::parse::<f64>) {
            total_seconds += seconds;
        }

        let Ok(duration) = Duration::try_from_secs_f64(total_seconds) else {
            continue;
        };

        if duration > largest {
            largest = duration;
        }
    }

    largest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn largest_pts_sums_the_packet_time_and_its_duration() {
        let probed = "1.400000|0.033333\n1.433333|0.033333\n";

        assert_eq!(largest_pts(probed), Duration::from_secs_f64(1.466666));
    }

    /// ffprobe reports a negative pts_time for the packets leading a
    /// timestamp discontinuity. Converting one panics, so the scan must skip
    /// it and keep reading the rest of the segment.
    #[test]
    fn a_negative_pts_is_skipped_rather_than_converted() {
        let probed = "-0.100000|0.033333\n2.000000|0.033333\n-4.500000|0.033333\n";

        assert_eq!(largest_pts(probed), Duration::from_secs_f64(2.033333));
    }

    /// Every packet in the segment can be negative, and the scan still has to
    /// return rather than abort.
    #[test]
    fn an_entirely_negative_segment_scans_to_zero() {
        let probed = "-9.000000|0.033333\n-4.500000|0.033333\n";

        assert_eq!(largest_pts(probed), Duration::ZERO);
    }

    /// A line carrying only a pts_time has no second field to read.
    #[test]
    fn a_line_without_a_duration_field_counts_its_pts_alone() {
        let probed = "1.400000\n";

        assert_eq!(largest_pts(probed), Duration::from_secs_f64(1.4));
    }

    #[test]
    fn unparseable_and_empty_output_scans_to_zero() {
        assert_eq!(largest_pts("N/A|N/A\n\n|\ngarbage\n"), Duration::ZERO);
        assert_eq!(largest_pts(""), Duration::ZERO);
    }
}
