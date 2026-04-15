use ersatztv_playout::playout::Playout;
use schemars::schema_for;

fn main() {
    let schema = schema_for!(Playout);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
