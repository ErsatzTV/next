mod error;

use ersatztv_playout::playout::PlayoutItemSource;
use ffpipeline::{pipeline, probe};

use crate::error::ChannelError;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), ChannelError> {
    // get playout JSON path
    let path = std::env::args()
        .nth(1)
        .ok_or(ChannelError::PlayoutJsonRequired)?;

    // load playout JSON
    let playout_result = ersatztv_playout::playout::load(path.as_str())?;

    // find current item
    let current_item = playout_result
        .playout
        .items
        .iter()
        .nth(0)
        .ok_or(ChannelError::PlayoutJsonNoItem)?;

    let current_source = current_item
        .source
        .clone()
        .ok_or(ChannelError::PlayoutJsonSingleSourceRequired)?;

    match current_source {
        PlayoutItemSource::Local { path } => {
            // probe current item
            let probe_result = probe::probe(path.as_str())?;
            println!("probe result:");
            println!("{probe_result}");
            println!();

            // generate pipeline
            let pipeline_result =
                pipeline::generate_pipeline(probe_result, String::from("/tmp/hls/live.m3u8"))?;
            println!("pipeline result:");
            println!("{pipeline_result}");
            println!();

            // TODO: stream current item
            Ok(())
        }
        _ => Err(ChannelError::PlayoutJsonLocalSourceRequired),
    }
}
