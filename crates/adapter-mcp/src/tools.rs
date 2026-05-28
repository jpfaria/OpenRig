//! One MCP tool per `Command` variant. Schema from `application::command_schema`
//! (single source of truth). A tool call rebuilds the externally-tagged
//! `Command` JSON (`{ "<Variant>": <args> }`), deserializes, and submits to
//! the bridge.

use std::sync::Arc;

use anyhow::Result;
use application::bridge::CommandBridge;
use application::command::Command;
use application::command_schema::{
    command_variant_names, command_variant_schema, tool_name, variant_from_tool_name,
};
use application::event::Event;
use rmcp::model::Tool;
use serde_json::{json, Map, Value};

/// Every variant as an MCP [`Tool`] with an auto-derived input schema.
pub fn tools() -> Vec<Tool> {
    command_variant_names()
        .iter()
        .map(|variant| {
            let schema = command_variant_schema(variant);
            let obj: Map<String, Value> = schema.as_object().cloned().unwrap_or_else(|| Map::new());
            Tool::new(
                tool_name(variant),
                format!("OpenRig command: {variant}"),
                Arc::new(obj),
            )
        })
        .collect()
}

/// Pure mapping: a tool name + JSON args → a typed `Command`. Resolves the
/// snake_case tool name to its `Command` variant, then delegates to the
/// single source of truth for "(variant, args) → Command",
/// `command_schema::command_from_variant` (the same builder `adapter-midi`
/// uses) — no parallel reconstruction of the externally-tagged form.
pub fn build_command(tool: &str, args: Value) -> Result<Command> {
    let variant =
        variant_from_tool_name(tool).ok_or_else(|| anyhow::anyhow!("unknown tool: {tool}"))?;
    // serde externally-tagged: unit variant = bare string `"Variant"`;
    // struct variant = `{ "Variant": <args> }`. MCP clients send `{}` for
    // no-arg tools, which serde rejects for a unit variant.
    let tagged = if application::command_schema::is_unit_variant(variant) {
        Value::String(variant.to_string())
    } else {
        json!({ variant: args })
    };
    serde_json::from_value(tagged).map_err(|e| anyhow::anyhow!("invalid arguments for {tool}: {e}"))
}

/// Map an incoming tool call to a `Command`, submit it over the bridge, and
/// await the resulting events.
pub async fn dispatch_tool(bridge: &CommandBridge, tool: &str, args: Value) -> Result<Vec<Event>> {
    let cmd = build_command(tool, args)?;
    let rx = bridge.submit(cmd);
    rx.await
        .map_err(|_| anyhow::anyhow!("frontend dropped the bridge"))?
        .map_err(|e| anyhow::anyhow!(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::command_schema::command_variant_names;

    /// `Command` has exactly this many variants. If you add/remove one,
    /// update this AND ensure its payload types derive `JsonSchema`
    /// (otherwise the variant silently drops from the schema → no tool).
    ///
    /// #513 / #493 bumped this from 44 → 49 with `SaveMidiDevices`,
    /// `SaveMidiMapping`, `StartMidiLearn`, `StopMidiLearn`, and
    /// `PublishMidiEvent`. Each adds one MCP tool automatically via
    /// `command_schema` (single source of truth — the `Command` enum).
    /// #513 (paths overrides) bumped to 51 with `SetPresetsPath` and
    /// `SetPluginsPath`. #561 bumped to 52 with `ReloadPluginCatalog`,
    /// then to 54 with `LoadPlugin` and `UnloadPlugin` (expanded scope:
    /// per-plugin load / unload). #548 bumped to 58 with
    /// `SelectActiveChainRelative`, `SelectActiveBlockRelative`,
    /// `SetCompactViewEnabled`, `ToggleActiveBlockNeighborEnabled`.
    /// #576 bumped to 59 with `RenderChain` — offline render via the
    /// command bus so every transport adapter (MCP/gRPC/…) inherits it.
    /// #582 bumped to 60 with `SetEvaluationsPath` — third
    /// system-paths override alongside `SetPresetsPath`/`SetPluginsPath`.
    const COMMAND_VARIANT_COUNT: usize = 60;

    #[test]
    fn parity_guard_every_command_variant_is_a_tool() {
        // Honest guard: the schema-derived tool set must cover ALL Command
        // variants, not just the schemars-describable subset. Catches the
        // #[schemars(skip)] regression (issue #489).
        assert_eq!(
            command_variant_names().len(),
            COMMAND_VARIANT_COUNT,
            "schema-derived variants != Command variants — a payload type \
             is missing JsonSchema (see #489)"
        );
        assert_eq!(tools().len(), COMMAND_VARIANT_COUNT);
        for t in tools() {
            assert!(variant_from_tool_name(&t.name).is_some(), "{}", t.name);
        }
        // Spot-check variants that were #[schemars(skip)]'d before #489.
        for v in [
            "AddChain",
            "ConfigureChain",
            "SaveChain",
            "LoadProject",
            "CreateProject",
            "SaveAudioSettings",
        ] {
            assert!(
                command_variant_names().contains(&v),
                "{v} missing from schema — JsonSchema not derived on its payload"
            );
        }
    }

    #[test]
    fn build_command_maps_unit_variant() {
        let cmd = build_command("save_project", Value::Null).unwrap();
        assert!(matches!(cmd, Command::SaveProject));
    }

    #[test]
    fn build_command_unit_variant_with_empty_object_args() {
        // MCP clients send `arguments: {}` for a no-arg tool; serde's
        // externally-tagged unit variant rejects a map, so build_command
        // must emit the bare string for unit variants.
        let cmd = build_command("save_project", serde_json::json!({})).unwrap();
        assert!(matches!(cmd, Command::SaveProject));
    }

    #[test]
    fn build_command_maps_struct_variant() {
        let cmd = build_command(
            "update_project_name",
            serde_json::json!({ "name": "Rig X" }),
        )
        .unwrap();
        match cmd {
            Command::UpdateProjectName { name } => assert_eq!(name, "Rig X"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn build_command_rejects_unknown_tool() {
        assert!(build_command("nope", Value::Null).is_err());
    }

    #[test]
    fn build_command_is_command_from_variant_single_source() {
        // The MCP tool path and the shared `command_schema::command_from_variant`
        // (also used by `adapter-midi`) MUST produce the identical `Command`
        // for equivalent input — one source of truth, not two parallel
        // reconstructions. Locks the dedup against future drift.
        use application::command_schema::{command_from_variant, variant_from_tool_name};
        for (tool, args) in [
            ("save_project", serde_json::json!({})),
            ("update_project_name", serde_json::json!({ "name": "X" })),
            (
                "toggle_block_enabled",
                serde_json::json!({ "chain": "chain:a", "block": "block:b" }),
            ),
        ] {
            let via_tool = build_command(tool, args.clone()).unwrap();
            let variant = variant_from_tool_name(tool).unwrap();
            let via_variant = command_from_variant(variant, args).unwrap();
            assert_eq!(
                serde_json::to_value(&via_tool).unwrap(),
                serde_json::to_value(&via_variant).unwrap(),
                "tool {tool}: MCP build_command diverged from the shared builder"
            );
        }
    }
}
