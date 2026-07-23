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
            let obj: Map<String, Value> = schema.as_object().cloned().unwrap_or_else(Map::new);
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
#[path = "tools_tests.rs"]
mod tests;
