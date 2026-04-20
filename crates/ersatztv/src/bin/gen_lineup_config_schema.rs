use ersatztv::config::LineupConfig;
use schemars::schema_for;

fn main() {
    let schema = schema_for!(LineupConfig);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
