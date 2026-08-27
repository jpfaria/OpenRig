//! Responsibility: walks the derived JSON schema of the command enum.

use crate::command::Command;
use schemars::schema_for;
use serde_json::Value;

pub(crate) fn command_root_schema() -> Value {
    serde_json::to_value(schema_for!(Command)).expect("Command schema serializes")
}

/// Pull the variant name out of one `oneOf`/`anyOf` entry, whether it is a
/// struct variant (`{ "required": ["Name"], "properties": { "Name": {...} } }`)
/// or a unit variant (`{ "enum": ["Name"] }` / `{ "const": "Name" }`).
pub(crate) fn entry_variant_name(entry: &Value) -> Option<String> {
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

pub(crate) fn branches(schema: &Value) -> Option<&Vec<Value>> {
    schema["oneOf"]
        .as_array()
        .or_else(|| schema["anyOf"].as_array())
}

/// The document's definition map, whichever draft `schemars` emitted it under.
pub(crate) fn definitions(root: &Value) -> Option<&serde_json::Map<String, Value>> {
    root.get("definitions")
        .or_else(|| root.get("$defs"))
        .and_then(Value::as_object)
}

/// Follow one `anyOf` entry of the untagged `Command` root to the sub-enum it
/// names, accepting both the bare `{"$ref": …}` and the
/// `{"allOf": [{"$ref": …}]}` shape `schemars` emits when the variant carries
/// extra metadata.
pub(crate) fn resolve_ref<'a>(root: &'a Value, entry: &Value) -> Option<&'a Value> {
    let reference = entry["$ref"]
        .as_str()
        .or_else(|| entry["allOf"][0]["$ref"].as_str())?;
    let name = reference.rsplit('/').next()?;
    definitions(root)?.get(name)
}

/// Push one entry per command, splitting the single string-`enum` entry
/// `schemars` folds *all* of an enum's unit variants into
/// (`{"enum":["SaveProject","CloseProject",…]}`) back into one entry each, so
/// every unit command keeps its own tool.
pub(crate) fn push_leaf(out: &mut Vec<Value>, entry: &Value) {
    match entry["enum"].as_array() {
        Some(names) if names.len() > 1 => out.extend(
            names
                .iter()
                .map(|n| serde_json::json!({ "type": "string", "enum": [n] })),
        ),
        _ => out.push(entry.clone()),
    }
}

/// Leaf variant entries of `Command`, one per command.
///
/// `Command` is `#[serde(untagged)]` over per-domain sub-enums, so the root
/// schema is `anyOf: [{$ref: BlockCommand}, …]` and the tool surface lives one
/// level down. Each `$ref` is resolved against the same document and its own
/// `oneOf` spliced in, flattening back to the pre-split, one-entry-per-command
/// list every adapter expects. Entries that are not sub-enum refs are kept
/// as-is so a future inline variant still shows up.
pub(crate) fn variant_entries(root: &Value) -> Vec<Value> {
    let Some(top) = branches(root) else {
        return Vec::new();
    };
    let mut leaves = Vec::new();
    for entry in top {
        match resolve_ref(root, entry).and_then(branches) {
            Some(inner) => inner.iter().for_each(|e| push_leaf(&mut leaves, e)),
            None => push_leaf(&mut leaves, entry),
        }
    }
    leaves
}
