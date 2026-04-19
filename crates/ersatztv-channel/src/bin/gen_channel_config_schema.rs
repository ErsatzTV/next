use ersatztv_channel::config::ChannelConfig;
use schemars::schema_for;

fn main() {
    let schema = schema_for!(ChannelConfig);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
