//! Phase 1 red-first test for SelectionState (issue #548).
//!
//! Validates the public shape of `application::SelectionState`:
//! - exists, implements `Default`
//! - has two public `Option<...>` fields: `active_chain`, `active_block`
//! - `clear_chain()` clears both (block belongs to chain — keeping a
//!   block without a chain would be an invariant violation).

use application::SelectionState;

#[test]
fn selection_state_default_is_empty() {
    let s: SelectionState = SelectionState::default();
    assert!(
        s.active_chain.is_none(),
        "default active_chain must be None"
    );
    assert!(
        s.active_block.is_none(),
        "default active_block must be None"
    );
}

#[test]
fn clearing_active_chain_also_clears_active_block() {
    let mut s = SelectionState {
        active_chain: Some("rig:guitar".to_string()),
        active_block: Some("blk_1".to_string()),
        ..Default::default()
    };

    s.clear_chain();

    assert!(
        s.active_chain.is_none(),
        "clear_chain must clear active_chain"
    );
    assert!(
        s.active_block.is_none(),
        "clear_chain must clear active_block (block belongs to chain)"
    );
}

#[test]
fn clearing_block_does_not_clear_chain() {
    let mut s = SelectionState {
        active_chain: Some("rig:guitar".to_string()),
        active_block: Some("blk_1".to_string()),
        ..Default::default()
    };

    s.clear_block();

    assert_eq!(
        s.active_chain.as_deref(),
        Some("rig:guitar"),
        "clearing block must NOT touch the chain"
    );
    assert!(
        s.active_block.is_none(),
        "clear_block must clear active_block"
    );
}
