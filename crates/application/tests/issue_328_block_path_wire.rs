//! #328 — the optional split `path` on `AddBlock` / `InsertPrebuiltBlock` /
//! `MoveBlock` is part of the wire format MCP, MIDI maps and gRPC speak.
//!
//! Two kinds of pin: a payload written before #328 (no `path`) parses and
//! serializes back byte-identical, and a payload WITH a path keeps it.

use application::command::Command;
use serde_json::{json, Value};

fn round_trip(wire: &str) -> Value {
    let parsed: Command = serde_json::from_str(wire).expect("wire format parses");
    serde_json::to_value(&parsed).expect("Command serializes")
}

/// Characterization pin (green before and after).
#[test]
fn add_block_without_path_serializes_exactly_as_before() {
    let wire = r#"{"AddBlock":{"chain":"c1","kind":"gain","model_id":"fuzz_ge","position":0}}"#;
    let parsed: Command = serde_json::from_str(wire).expect("a pre-#328 AddBlock parses");
    assert_eq!(serde_json::to_string(&parsed).expect("serializes"), wire);
}

#[test]
fn add_block_keeps_the_split_path_it_was_sent() {
    let out = round_trip(
        r#"{"AddBlock":{"chain":"c1","kind":"gain","model_id":"fuzz_ge","position":0,"path":{"split":"s1","path":1}}}"#,
    );
    assert_eq!(out["AddBlock"]["path"], json!({ "split": "s1", "path": 1 }));
}

#[test]
fn insert_prebuilt_block_keeps_the_split_path_it_was_sent() {
    let out = round_trip(
        r#"{"InsertPrebuiltBlock":{"chain":"c1","block":{"id":"b1","enabled":true,"kind":{"Core":{"effect_type":"gain","model":"fuzz_ge","params":{"values":{}}}}},"position":0,"path":{"split":"s1","path":0}}}"#,
    );
    assert_eq!(
        out["InsertPrebuiltBlock"]["path"],
        json!({ "split": "s1", "path": 0 })
    );
}

/// Characterization pin (green before and after).
#[test]
fn move_block_without_path_serializes_exactly_as_before() {
    let wire = r#"{"MoveBlock":{"chain":"c1","block":"b1","new_position":2}}"#;
    let parsed: Command = serde_json::from_str(wire).expect("a pre-#328 MoveBlock parses");
    assert_eq!(serde_json::to_string(&parsed).expect("serializes"), wire);
}

#[test]
fn move_block_keeps_the_destination_path_it_was_sent() {
    let out = round_trip(
        r#"{"MoveBlock":{"chain":"c1","block":"b1","new_position":2,"path":{"split":"s1","path":1}}}"#,
    );
    assert_eq!(
        out["MoveBlock"]["path"],
        json!({ "split": "s1", "path": 1 })
    );
}
