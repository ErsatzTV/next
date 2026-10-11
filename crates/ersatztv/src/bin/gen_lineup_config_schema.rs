use ersatztv::config::{LineupConfig, SCHEMA};
use schemars::schema_for;

fn main() {
    let mut schema = schema_for!(LineupConfig);
    schema.insert(String::from("$id"), SCHEMA.uri().into());
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
