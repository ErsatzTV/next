use ffpipeline::probe;

pub fn main() {
    let path = std::env::args().nth(1);

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
            eprintln!("path not given");
            std::process::exit(1);
        }
    }
}
