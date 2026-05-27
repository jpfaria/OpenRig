//! Phase 3a red-first test (issue #548):
//! the 3 new Commands required by Phase 0 audit must exist as variants
//! of `Command` so MIDI slots (Phase 3b+) can dispatch them and MCP/gRPC
//! discover them automatically (Command enum is the parity source).

use application::command::Command;

#[test]
fn select_active_chain_relative_variant_exists() {
    let cmd = Command::SelectActiveChainRelative { delta: 1 };
    match cmd {
        Command::SelectActiveChainRelative { delta } => assert_eq!(delta, 1),
        _ => panic!("expected SelectActiveChainRelative variant"),
    }
}

#[test]
fn select_active_block_relative_variant_exists() {
    let cmd = Command::SelectActiveBlockRelative { delta: -2 };
    match cmd {
        Command::SelectActiveBlockRelative { delta } => assert_eq!(delta, -2),
        _ => panic!("expected SelectActiveBlockRelative variant"),
    }
}

#[test]
fn set_compact_view_enabled_variant_exists() {
    let cmd = Command::SetCompactViewEnabled { enabled: true };
    match cmd {
        Command::SetCompactViewEnabled { enabled } => assert!(enabled),
        _ => panic!("expected SetCompactViewEnabled variant"),
    }
}
