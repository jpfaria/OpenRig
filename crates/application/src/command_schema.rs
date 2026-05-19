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

    mod command_from_variant {
        use super::super::command_from_variant;
        use crate::command::Command;

        #[test]
        fn builds_unit_variant_with_empty_args() {
            let cmd = command_from_variant("SaveProject", serde_json::json!({})).unwrap();
            assert!(matches!(cmd, Command::SaveProject));
        }

        #[test]
        fn builds_struct_variant_from_args() {
            let cmd = command_from_variant(
                "ToggleBlockEnabled",
                serde_json::json!({ "chain": "chain:a", "block": "block:b" }),
            )
            .unwrap();
            match cmd {
                Command::ToggleBlockEnabled { chain, block } => {
                    assert_eq!(chain.0, "chain:a");
                    assert_eq!(block.0, "block:b");
                }
                other => panic!("unexpected: {other:?}"),
            }
        }

        #[test]
        fn builds_number_variant_with_scaled_value() {
            let cmd = command_from_variant(
                "SetBlockParameterNumber",
                serde_json::json!({
                    "chain": "chain:a", "block": "block:b",
                    "path": "gain", "value": 75.0
                }),
            )
            .unwrap();
            match cmd {
                Command::SetBlockParameterNumber { value, path, .. } => {
                    assert_eq!(path, "gain");
                    assert!((value - 75.0).abs() < 1e-9);
                }
                other => panic!("unexpected: {other:?}"),
            }
        }

        #[test]
        fn rejects_unknown_command() {
            let err = command_from_variant("Nope", serde_json::json!({}))
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown command"), "{err}");
        }

        #[test]
        fn rejects_args_not_matching_schema() {
            // `chain` must be a string id; a number fails the schema.
            let err = command_from_variant(
                "ToggleBlockEnabled",
                serde_json::json!({ "chain": 0, "block": "block:b" }),
            )
            .unwrap_err()
            .to_string();
            assert!(err.contains("invalid arguments"), "{err}");
        }
    }

    /// Adapter-agnostic parity guard. The previous guard
    /// (`parity_guard_one_tool_per_command_variant` in `adapter-mcp`) only
    /// proved the MCP projection. This proves the contract every adapter
    /// shares — MCP, `adapter-midi`, and the future gRPC adapter all reach
    /// `Command` through `command_from_variant` / the schema-derived names.
    /// If a `Command` variant loses `JsonSchema` (silently dropping it from
    /// the surface) or stops round-tripping, this fails next to `Command`
    /// itself, not buried in one transport crate.
    mod adapter_agnostic_parity {
        use super::super::{
            command_from_variant, command_variant_names, tool_name, variant_from_tool_name,
        };

        #[test]
        fn every_command_variant_is_reachable_by_all_adapters() {
            let names = command_variant_names();
            assert!(!names.is_empty(), "no Command variants in the schema");
            for v in names {
                // 1. snake_case tool name round-trips (MCP naming).
                assert_eq!(
                    variant_from_tool_name(&tool_name(v)),
                    Some(*v),
                    "tool-name does not round-trip for {v}"
                );
                // 2. the single builder every adapter uses honors the
                //    variant *name*. Struct variants legitimately fail with
                //    "invalid arguments" given empty args — that is fine; the
                //    contract is that the variant is never "unknown", i.e.
                //    no adapter is blind to it.
                let err = command_from_variant(v, serde_json::json!({}))
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_default();
                assert!(
                    !err.contains("unknown command"),
                    "{v}: unreachable via command_from_variant — MCP/MIDI/gRPC \
                     would all be blind to it (lost JsonSchema?)"
                );
            }
        }
    }
}


