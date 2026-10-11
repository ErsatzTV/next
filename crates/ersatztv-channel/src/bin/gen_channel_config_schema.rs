use ersatztv_channel::config::{ChannelConfig, SCHEMA};
use schemars::schema_for;

fn main() {
    let mut schema = schema_for!(ChannelConfig);
    schema.insert(String::from("$id"), SCHEMA.uri().into());
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
