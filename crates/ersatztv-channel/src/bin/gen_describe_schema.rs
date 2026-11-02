use ersatztv_channel::describe::Describe;
use schemars::schema_for;

fn main() {
    let schema = schema_for!(Describe);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
