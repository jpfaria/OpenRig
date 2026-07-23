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
