mod error;

use ffpipeline::probe;

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
    let result = probe::probe(path.as_str())?;
    println!("{result}");
    Ok(())

    // TODO: generate pipeline

    // TODO: stream current item
}
