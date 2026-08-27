//! Responsibility: derives the tool schema a transport advertises from the command enum.
//! Bridges the `schemars`-derived schema of [`crate::command::Command`] into
//! a per-variant tool surface. Single source of truth = the `Command` enum;
//! no hand-written JSON Schema. Consumed by `adapter-mcp` to expose one MCP
//! tool per command with an auto-derived input schema.

use std::sync::OnceLock;

use serde_json::Value;

use crate::command::Command;
pub(crate) use crate::schema_walk::{
    command_root_schema, definitions, entry_variant_name, variant_entries,
};
pub use crate::tool_names::{tool_name, variant_from_tool_name};

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
    // Keep the key `schemars` actually used — the `$ref`s point at it by name.
    let defs_key = if root.get("definitions").is_some() {
        "definitions"
    } else {
        "$defs"
    };
    let defs = definitions(&root).cloned();
    for entry in variant_entries(&root) {
        if entry_variant_name(&entry).as_deref() == Some(variant) {
            if let Some(args) = entry["properties"].get(variant) {
                let mut args = args.clone();
                if let (Some(obj), Some(defs)) = (args.as_object_mut(), defs) {
                    obj.insert(defs_key.to_string(), Value::Object(defs));
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
