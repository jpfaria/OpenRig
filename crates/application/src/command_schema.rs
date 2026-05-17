//! Bridges the `schemars`-derived schema of [`crate::command::Command`] into
//! a per-variant tool surface. Single source of truth = the `Command` enum;
//! no hand-written JSON Schema. Consumed by `adapter-mcp` to expose one MCP
//! tool per command with an auto-derived input schema.

use std::sync::OnceLock;

use schemars::schema_for;
use serde_json::Value;

use crate::command::Command;

fn command_root_schema() -> Value {
    serde_json::to_value(schema_for!(Command)).expect("Command schema serializes")
}

/// Pull the variant name out of one `oneOf`/`anyOf` entry, whether it is a
/// struct variant (`{ "required": ["Name"], "properties": { "Name": {...} } }`)
/// or a unit variant (`{ "enum": ["Name"] }` / `{ "const": "Name" }`).
fn entry_variant_name(entry: &Value) -> Option<String> {
    if let Some(name) = entry["required"]
        .as_array()
        .and_then(|r| r.first())
        .and_then(Value::as_str)
    {
        return Some(name.to_string());
    }
    if let Some(name) = entry["enum"]
        .as_array()
        .and_then(|e| e.first())
        .and_then(Value::as_str)
    {
        return Some(name.to_string());
    }
    entry["const"].as_str().map(str::to_string)
}

fn variant_entries(root: &Value) -> Vec<Value> {
    root["oneOf"]
        .as_array()
        .or_else(|| root["anyOf"].as_array())
        .cloned()
        .unwrap_or_default()
}

/// All `Command` variant names, derived once from the static schema.
pub fn command_variant_names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            let root = command_root_schema();
            variant_entries(&root)
                .iter()
                .filter_map(entry_variant_name)
                .map(|s| -> &'static str { Box::leak(s.into_boxed_str()) })
                .collect()
        })
        .as_slice()
}

/// Object schema for a single variant's arguments (the value side of the
/// externally-tagged pair). Unit variants get an empty object schema.
pub fn command_variant_schema(variant: &str) -> Value {
    let root = command_root_schema();
    for entry in variant_entries(&root) {
        if entry_variant_name(&entry).as_deref() == Some(variant) {
            if let Some(args) = entry["properties"].get(variant) {
                return args.clone();
            }
            break;
        }
    }
    serde_json::json!({ "type": "object", "properties": {}, "required": [] })
}

/// `SetBlockParameterNumber` -> `set_block_parameter_number`.
pub fn tool_name(variant: &str) -> String {
    let mut s = String::with_capacity(variant.len() + 8);
    for (i, ch) in variant.char_indices() {
        if ch.is_uppercase() && i != 0 {
            s.push('_');
        }
        s.push(ch.to_ascii_lowercase());
    }
    s
}

/// Reverse of [`tool_name`]; `None` if it matches no `Command` variant.
pub fn variant_from_tool_name(tool: &str) -> Option<&'static str> {
    command_variant_names()
        .iter()
        .copied()
        .find(|v| tool_name(v) == tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_a_tool_name_and_schema() {
        let names = command_variant_names();
        assert!(names.contains(&"SaveProject"));
        assert!(names.contains(&"SetBlockParameterNumber"));
        for v in names {
            let tn = tool_name(v);
            assert_eq!(variant_from_tool_name(&tn), Some(*v));
            let schema = command_variant_schema(v);
            assert_eq!(schema["type"], "object", "variant {v} schema not object");
        }
    }

    #[test]
    fn tool_name_is_snake_case() {
        assert_eq!(
            tool_name("SetBlockParameterNumber"),
            "set_block_parameter_number"
        );
        assert_eq!(tool_name("SaveProject"), "save_project");
    }
}
