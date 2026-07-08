//! Task 4 — MCP parity for the I/O binding registry commands (issue #716).
//!
//! The MCP tool layer is auto-derived from `command_variant_names()`.
//! These tests pin that the three io-binding variants are present in the
//! schema catalogue and that `command_from_variant` can round-trip each one
//! from the JSON payload an MCP client would send.
//!
//! RED-first: written before the variants existed in command_schema so the
//! tests fail with "create_io_binding is not a known variant".

use application::command::Command;
use application::command_schema::{command_from_variant, command_variant_names, tool_name};

fn sample_binding_json() -> serde_json::Value {
    serde_json::json!({
        "binding": {
            "id": "main",
            "name": "Main Rig",
            "inputs": [
                {
                    "name": "Guitar In",
                    "device_id": "hw:0,0",
                    "mode": "mono",
                    "channels": [0]
                }
            ],
            "outputs": []
        }
    })
}

// ---------------------------------------------------------------------------
// CreateIoBinding
// ---------------------------------------------------------------------------

#[test]
fn create_io_binding_is_a_known_command_variant() {
    let names = command_variant_names();
    assert!(
        names.contains(&"CreateIoBinding"),
        "command_variant_names() must contain CreateIoBinding (got: {names:?})"
    );
}

#[test]
fn create_io_binding_tool_name_follows_snake_case_convention() {
    assert_eq!(tool_name("CreateIoBinding"), "create_io_binding");
}

#[test]
fn create_io_binding_deserializes_from_mcp_payload() {
    let cmd = command_from_variant("CreateIoBinding", sample_binding_json())
        .expect("command_from_variant must parse CreateIoBinding payload");
    match cmd {
        Command::CreateIoBinding { binding } => {
            assert_eq!(binding.id, "main");
            assert_eq!(binding.name, "Main Rig");
            assert_eq!(binding.inputs.len(), 1);
            assert_eq!(binding.inputs[0].device_id.0, "hw:0,0");
        }
        other => panic!("expected CreateIoBinding, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// UpdateIoBinding
// ---------------------------------------------------------------------------

#[test]
fn update_io_binding_is_a_known_command_variant() {
    let names = command_variant_names();
    assert!(
        names.contains(&"UpdateIoBinding"),
        "command_variant_names() must contain UpdateIoBinding (got: {names:?})"
    );
}

#[test]
fn update_io_binding_tool_name_follows_snake_case_convention() {
    assert_eq!(tool_name("UpdateIoBinding"), "update_io_binding");
}

#[test]
fn update_io_binding_deserializes_from_mcp_payload() {
    let cmd = command_from_variant("UpdateIoBinding", sample_binding_json())
        .expect("command_from_variant must parse UpdateIoBinding payload");
    match cmd {
        Command::UpdateIoBinding { binding } => {
            assert_eq!(binding.id, "main");
        }
        other => panic!("expected UpdateIoBinding, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// DeleteIoBinding
// ---------------------------------------------------------------------------

#[test]
fn delete_io_binding_is_a_known_command_variant() {
    let names = command_variant_names();
    assert!(
        names.contains(&"DeleteIoBinding"),
        "command_variant_names() must contain DeleteIoBinding (got: {names:?})"
    );
}

#[test]
fn delete_io_binding_tool_name_follows_snake_case_convention() {
    assert_eq!(tool_name("DeleteIoBinding"), "delete_io_binding");
}

#[test]
fn delete_io_binding_deserializes_from_mcp_payload() {
    let cmd = command_from_variant("DeleteIoBinding", serde_json::json!({ "id": "main" }))
        .expect("command_from_variant must parse DeleteIoBinding payload");
    match cmd {
        Command::DeleteIoBinding { id } => {
            assert_eq!(id, "main");
        }
        other => panic!("expected DeleteIoBinding, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Adapter-MCP parity: tool count matches command variant count
// ---------------------------------------------------------------------------
//
// This test is the live guard that prevents a future Command variant being
// added to command.rs without automatically showing up in the MCP tool
// listing.  The tools.rs `parity_guard_every_command_variant_is_a_tool`
// test is the canonical pin; this file adds the three io-binding
// variants to that surface.
//
// Since adapter-mcp is a separate crate, the tool-count guard lives in
// adapter-mcp/src/tools.rs and is exercised by `cargo test -p adapter-mcp`.
// We do NOT duplicate that guard here; instead we verify the three
// variants are reachable through command_from_variant so that if the
// guard already exists AND these variants are registered, both sides are
// green.

#[test]
fn all_three_io_binding_variants_round_trip_via_command_schema() {
    // Aggregate check: if any of the three is missing the test fails with
    // a clear listing of which variants are absent.
    let names = command_variant_names();
    let required = ["CreateIoBinding", "UpdateIoBinding", "DeleteIoBinding"];
    let missing: Vec<_> = required.iter().filter(|&&r| !names.contains(&r)).collect();
    assert!(
        missing.is_empty(),
        "command_variant_names() is missing io-binding variants: {missing:?}\n\
         These must appear so the MCP adapter auto-exposes them as tools.\n\
         Known variants: {names:?}"
    );
}
