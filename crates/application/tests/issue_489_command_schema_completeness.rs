//! Issue #489: every `Command` variant exposed as an MCP/gRPC tool must emit a
//! self-contained JsonSchema for its payload. Today, variants whose payload
//! types live in `crates/project` / `crates/block-core` carry `#[schemars(skip)]`
//! (or reference types that don't derive `JsonSchema`), so the schema for the
//! struct field is a `$ref` whose target is never emitted into `definitions`.
//!
//! Live symptom from a real MCP client on `develop` today:
//! ```text
//! add_chain → "invalid arguments for add_chain: invalid type: string
//!              \"{...chain JSON...}\", expected struct Chain"
//! ```
//! Cause: the tool's input schema declares `chain` as `$ref:
//! #/definitions/Chain`, but `definitions` does not include `Chain`. Clients
//! that can't resolve the `$ref` fall back to treating the field as opaque
//! (sending the struct as a JSON-encoded string), and the server's `serde`
//! deserializer then rejects the string token.
//!
//! This test pins the contract: every Command variant whose payload field is a
//! `$ref` must include the resolvable target in the same schema document.

use application::command_schema::{command_variant_names, command_variant_schema};
use serde_json::Value;

/// Collect every `$ref` value from a JSON Schema fragment.
fn collect_refs(node: &Value, out: &mut Vec<String>) {
    match node {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get("$ref") {
                out.push(r.clone());
            }
            for (_, v) in map {
                collect_refs(v, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_refs(item, out);
            }
        }
        _ => {}
    }
}

#[test]
fn issue_489_every_command_variant_schema_has_resolvable_refs() {
    let mut broken: Vec<(String, Vec<String>)> = Vec::new();
    for name in command_variant_names() {
        let schema = command_variant_schema(name);
        let mut refs = Vec::new();
        collect_refs(&schema, &mut refs);
        let mut unresolved = Vec::new();
        for r in refs {
            let path = r.trim_start_matches('#');
            if schema.pointer(path).is_none() {
                unresolved.push(r);
            }
        }
        if !unresolved.is_empty() {
            broken.push((name.to_string(), unresolved));
        }
    }
    assert!(
        broken.is_empty(),
        "Issue #489: these Command variant schemas contain `$ref`s whose \
         targets are not present in the emitted schema (clients can't resolve \
         them and fall back to opaque strings, breaking deserialization):\n{}",
        broken
            .iter()
            .map(|(name, refs)| format!("  {name}: {refs:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn issue_489_add_chain_schema_describes_chain_payload_end_to_end() {
    // The narrowest pin on the bug we saw live: AddChain.chain has to be a
    // resolvable struct schema, not a dangling `$ref` (which collapses to
    // an opaque/string field on the client side).
    let schema = command_variant_schema("AddChain");
    let chain_field = schema
        .pointer("/properties/chain")
        .expect("AddChain schema must declare a `chain` field");
    if let Some(reference) = chain_field.get("$ref").and_then(Value::as_str) {
        let path = reference.trim_start_matches('#');
        assert!(
            schema.pointer(path).is_some(),
            "AddChain.chain `$ref` to `{}` is dangling -- the Chain definition \
             is not inlined in the emitted schema. Full schema: {}",
            reference,
            serde_json::to_string_pretty(&schema).unwrap_or_default()
        );
    } else {
        let kind = chain_field.get("type").and_then(Value::as_str);
        assert!(
            kind == Some("object")
                || chain_field.get("properties").is_some()
                || chain_field.get("allOf").is_some()
                || chain_field.get("anyOf").is_some(),
            "AddChain.chain must be a struct schema (object / inline / $ref \
             with resolvable target), got: {}",
            serde_json::to_string_pretty(chain_field).unwrap_or_default()
        );
    }
}
