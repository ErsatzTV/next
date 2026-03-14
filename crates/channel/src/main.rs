mod error;

use ffpipeline::{pipeline, probe};

use crate::error::ChannelError;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), ChannelError> {
    // TODO: find current item from playout JSON; for now, read as arg
    let path = std::env::args()
        .nth(1)
        .ok_or(ChannelError::PlayoutJsonUnsupported)?;

    // probe current item
    let probe_result = probe::probe(path.as_str())?;
    println!("probe result:");
    println!("{probe_result}");
    println!();

    // TODO: generate pipeline
    let pipeline_result = pipeline::generate_pipeline(probe_result)?;
    println!("pipeline result:");
    println!("{pipeline_result}");
    println!();

    // TODO: stream current item
    Ok(())
}
