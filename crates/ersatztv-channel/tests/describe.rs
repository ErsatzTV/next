use std::path::Path;
use std::process::Command;

use ersatztv_channel::describe::Describe;

#[test]
fn prints_describe_document() {
    let output = Command::new(env!("CARGO_BIN_EXE_ersatztv-channel"))
        .arg("describe")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let printed: Describe = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(printed, Describe::new());
}

#[test]
fn committed_schema_is_current() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/describe.json");
    let committed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let generated = serde_json::to_value(schemars::schema_for!(Describe)).unwrap();
    assert_eq!(
        committed, generated,
        "regenerate: cargo run --bin gen-describe-schema > schema/describe.json"
    );
}
