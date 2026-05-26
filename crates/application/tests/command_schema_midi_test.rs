//! Phase 3a red-first test (issue #548):
//! the 3 new MIDI Commands must show up in `command_schema` so the
//! adapter-mcp parity guard (and any future MCP tool dispatch) sees them
//! automatically. Without this, `parity_guard_every_command_variant_is_a_tool`
//! widens an already-existing gap (#489) by 3 more variants per
//! commit 597427569.

use application::command_schema::{
    command_from_variant, command_variant_names, tool_name,
};

#[test]
fn select_active_chain_relative_is_a_known_command() {
    let names = command_variant_names();
    assert!(
        names.contains(&"SelectActiveChainRelative"),
        "command_variant_names missing SelectActiveChainRelative"
    );
    assert_eq!(
        tool_name("SelectActiveChainRelative"),
        "select_active_chain_relative"
    );
    let cmd =
        command_from_variant("SelectActiveChainRelative", serde_json::json!({ "delta": 1 }))
            .expect("build from MCP payload");
    match cmd {
        application::command::Command::SelectActiveChainRelative { delta } => {
            assert_eq!(delta, 1);
        }
        other => panic!("expected SelectActiveChainRelative, got {other:?}"),
    }
}

#[test]
fn select_active_block_relative_is_a_known_command() {
    let names = command_variant_names();
    assert!(names.contains(&"SelectActiveBlockRelative"));
    assert_eq!(
        tool_name("SelectActiveBlockRelative"),
        "select_active_block_relative"
    );
    let cmd =
        command_from_variant("SelectActiveBlockRelative", serde_json::json!({ "delta": -2 }))
            .expect("build from MCP payload");
    match cmd {
        application::command::Command::SelectActiveBlockRelative { delta } => {
            assert_eq!(delta, -2);
        }
        other => panic!("expected SelectActiveBlockRelative, got {other:?}"),
    }
}

#[test]
fn set_compact_view_enabled_is_a_known_command() {
    let names = command_variant_names();
    assert!(names.contains(&"SetCompactViewEnabled"));
    assert_eq!(
        tool_name("SetCompactViewEnabled"),
        "set_compact_view_enabled"
    );
    let cmd =
        command_from_variant("SetCompactViewEnabled", serde_json::json!({ "enabled": true }))
            .expect("build from MCP payload");
    match cmd {
        application::command::Command::SetCompactViewEnabled { enabled } => {
            assert!(enabled);
        }
        other => panic!("expected SetCompactViewEnabled, got {other:?}"),
    }
}
