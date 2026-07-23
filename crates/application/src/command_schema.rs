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
///
/// The root schema's `definitions` map is copied into the returned subschema so
/// `$ref`s emitted by `schemars` (e.g. `#/definitions/Chain`, `ChainId`) resolve
/// against the same document. Without this copy, MCP/gRPC clients see dangling
/// refs and fall back to opaque/stringified payloads — which the server-side
/// `serde` deserializer then rejects with `"expected struct X, got string"`.
/// See issue #489.
pub fn command_variant_schema(variant: &str) -> Value {
    let root = command_root_schema();
    let definitions = root.get("definitions").cloned();
    for entry in variant_entries(&root) {
        if entry_variant_name(&entry).as_deref() == Some(variant) {
            if let Some(args) = entry["properties"].get(variant) {
                let mut args = args.clone();
                if let (Some(obj), Some(defs)) = (args.as_object_mut(), definitions) {
                    obj.insert("definitions".to_string(), defs);
                }
                return args;
            }
            break;
        }
    }
    serde_json::json!({ "type": "object", "properties": {}, "required": [] })
}

/// True if the variant carries no fields (serde externally-tagged unit
/// variant — serialized as the bare string `"Variant"`, not `{"Variant":…}`).
/// `schemars` emits these inside a string/`enum` entry, not as an object
/// entry with `properties.<Variant>`.
pub fn is_unit_variant(variant: &str) -> bool {
    let root = command_root_schema();
    for entry in variant_entries(&root) {
        if let Some(en) = entry["enum"].as_array() {
            if en.iter().filter_map(Value::as_str).any(|n| n == variant) {
                return true;
            }
        }
        if entry_variant_name(&entry).as_deref() == Some(variant) {
            return entry["properties"].get(variant).is_none();
        }
    }
    false
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

/// Build a typed [`Command`] from a `Command` variant name (PascalCase, as
/// written in `midi-map.yaml`) and its JSON arguments. Single source of truth
/// for "(name, args) → Command": reconstructs the externally-tagged form serde
/// expects — a bare string `"Variant"` for unit variants, `{ "Variant": args }`
/// otherwise.
///
/// # Errors
/// - the variant is not a `Command` variant;
/// - `args` does not match the variant's schema.
pub fn command_from_variant(variant: &str, args: Value) -> anyhow::Result<Command> {
    if !command_variant_names().contains(&variant) {
        anyhow::bail!("unknown command: {variant}");
    }
    let tagged = if is_unit_variant(variant) {
        Value::String(variant.to_string())
    } else {
        serde_json::json!({ variant: args })
    };
    serde_json::from_value(tagged)
        .map_err(|e| anyhow::anyhow!("invalid arguments for {variant}: {e}"))
}

#[cfg(test)]
#[path = "command_schema_tests.rs"]
mod tests;
