//! The schemas are the public contract and the docs site's reference is generated from their
//! `description`s, so every property and every enum value must have one.

use std::path::Path;

use serde_json::Value;

const SCHEMAS: &[&str] = &["channel_config.json", "lineup_config.json", "playout.json"];

fn collect_missing(node: &Value, path: &str, missing: &mut Vec<String>) {
    match node {
        Value::Object(map) => {
            let described = map.contains_key("description");

            if let Some(Value::Object(properties)) = map.get("properties") {
                for (name, property) in properties {
                    if property.get("description").is_none() {
                        missing.push(format!("{path}/properties/{name}"));
                    }
                }
            }

            // a plain `enum` can't describe each value, so the node's description must cover them
            if map.contains_key("enum") && !described {
                missing.push(format!("{path} (enum)"));
            }

            // documented enum values are `oneOf` branches with a `const`
            if let Some(Value::Array(branches)) = map.get("oneOf") {
                for (i, branch) in branches.iter().enumerate() {
                    if branch.get("const").is_some() && branch.get("description").is_none() {
                        missing.push(format!("{path}/oneOf/{i} (const)"));
                    }
                }
            }

            for (key, value) in map {
                collect_missing(value, &format!("{path}/{key}"), missing);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                collect_missing(item, &format!("{path}/{i}"), missing);
            }
        }
        _ => {}
    }
}

#[test]
fn every_property_and_enum_value_has_a_description() {
    let schema_folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema");

    let mut missing = Vec::new();
    for name in SCHEMAS {
        let text = std::fs::read_to_string(schema_folder.join(name)).unwrap();
        let schema: Value = serde_json::from_str(&text).unwrap();
        collect_missing(&schema, name, &mut missing);
    }

    assert!(
        missing.is_empty(),
        "schema nodes without a description:\n{}",
        missing.join("\n")
    );
}
