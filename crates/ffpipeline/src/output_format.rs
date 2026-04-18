use std::time::Duration;

use crate::ArgVec;
use crate::pipeline::{KEYFRAME_INTERVAL_SECONDS, OutputContext, SEGMENT_SECONDS};
use crate::video_codec::VideoCodec;

#[derive(Debug)]
pub enum OutputFormat {
    Hls {
        playlist: String,
        segment_template: String,
    },
}

impl OutputFormat {
    pub(crate) fn as_arg(&self, output_context: &OutputContext) -> ArgVec {
        let force_key_frames_expr = format!("expr:gte(t,n_forced*{KEYFRAME_INTERVAL_SECONDS})");
        let segment_seconds = format!("{SEGMENT_SECONDS}");
        let rounded_frame_rate = output_context
            .media_frame_rate
            .parsed_frame_rate
            .round_ties_even() as u32;

        // TODO: 1-second GOP for qsv
        let gop = format!("{}", rounded_frame_rate * KEYFRAME_INTERVAL_SECONDS);
        let keyint_min = format!("{}", rounded_frame_rate * KEYFRAME_INTERVAL_SECONDS);

        let mut args: ArgVec = Vec::new();

        match self {
            OutputFormat::Hls {
                segment_template, ..
            } => {
                if output_context.video_codec != VideoCodec::COPY {
                    args.extend(args![
                        "-g",
                        gop,
                        "-keyint_min",
                        keyint_min,
                        "-force_key_frames",
                        force_key_frames_expr,
                    ]);
                }

                args.extend(args![
                    "-f",
                    "hls",
                    "-hls_time",
                    segment_seconds,
                    "-hls_list_size",
                    "0",
                    "-segment_list_flags",
                    "+live",
                    "-hls_segment_filename",
                    segment_template.to_owned(),
                    "-hls_segment_type",
                    "mpegts",
                    "-hls_flags",
                    "program_date_time+omit_endlist+append_list+independent_segments",
                ]);

                match output_context.pts_offset {
                    Some(pts_offset) if pts_offset.duration > Duration::ZERO => {}
                    _ => args.extend(args![
                        "-hls_segment_options",
                        "mpegts_flags=+initial_discontinuity",
                    ]),
                }
            }
        }

        args
    }

    pub(crate) fn path(&self) -> String {
        match self {
            OutputFormat::Hls { playlist, .. } => playlist.clone(),
        }
    }
}
