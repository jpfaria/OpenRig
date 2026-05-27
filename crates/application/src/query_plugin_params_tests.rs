//! Red-first (#572) tests for `query::get_plugin_params`.
//!
//! See spec
//! `docs/superpowers/specs/2026-05-27-issue-572-mcp-block-plugin-params-design.md`.

use crate::query::get_plugin_params;

#[test]
fn get_plugin_params_unknown_id_returns_null_envelope() {
    // Mirrors `get_plugin`'s contract for unknown ids — the wire shape
    // an MCP / gRPC client gets when the plugin id has no match in the
    // catalog is a JSON envelope with `params: null` (consistent with
    // `{"plugin": null}` for `get_plugin`).
    let out = get_plugin_params("definitely-not-a-real-plugin-id-572");
    assert_eq!(out, "{\"params\": null}");
}
