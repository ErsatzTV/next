use ffpipeline::probe;

pub fn main() {
    // TODO: find current item from playout JSON; for now, read as arg
    let path = std::env::args().nth(1);

    // probe current item
    match path {
        Some(path) => match probe::probe(path.as_str()) {
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
            Ok(result) => {
                println!("{result}");
            }
        },
        None => {
            eprintln!("playout JSON is not yet supported; pass video file as arg");
            std::process::exit(1);
        }
    }

    // TODO: generate pipeline

    // TODO: stream current item
}
