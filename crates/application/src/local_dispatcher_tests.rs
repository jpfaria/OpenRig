//! Tests for `LocalDispatcher::dispatch(Command::ToggleBlockEnabled)`.
//!
//! Follows strict TDD: tests were written first (RED), then the implementation
//! was added to `local_dispatcher.rs` (GREEN).
//!
//! Attached to `lib.rs` via:
//! ```text
//! #[cfg(test)]
//! #[path = "local_dispatcher_tests.rs"]
//! mod local_dispatcher_tests;
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::session::ApplicationSession;

// ── helpers ──────────────────────────────────────────────────────────────────

fn make_core_block(id: &str, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".to_string(),
            model: "test_model".to_string(),
            params: ParameterSet::default(),
        }),
    }
}

fn make_session(chain_id: &str, block: AudioBlock) -> ApplicationSession {
    ApplicationSession {
        project: Project {
            name: None,
            device_settings: vec![],
            chains: vec![Chain {
                id: ChainId(chain_id.to_string()),
                description: None,
                instrument: "electric_guitar".to_string(),
                enabled: true,
                blocks: vec![block],
            }],
        },
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn toggle_block_enabled_flips_true_to_false_and_emits_event() {
    let session = make_session("chain_0", make_core_block("blk_0", true));
    let session_rc = Rc::new(RefCell::new(session));
    let dispatcher = LocalDispatcher {
        project_session: Rc::clone(&session_rc),
    };

    let result = dispatcher.dispatch(Command::ToggleBlockEnabled {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(
            &events[0],
            Event::BlockEnabledChanged {
                chain,
                block,
                enabled: false,
            }
            if chain.0 == "chain_0" && block.0 == "blk_0"
        ),
        "unexpected event: {:?}",
        events[0]
    );
    assert!(
        !session_rc.borrow().project.chains[0].blocks[0].enabled,
        "block should be disabled after toggle"
    );
}

#[test]
fn toggle_block_enabled_non_existent_block_returns_err_no_mutation() {
    let session = make_session("chain_0", make_core_block("blk_0", true));
    let session_rc = Rc::new(RefCell::new(session));
    let dispatcher = LocalDispatcher {
        project_session: Rc::clone(&session_rc),
    };

    let result = dispatcher.dispatch(Command::ToggleBlockEnabled {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    assert!(
        session_rc.borrow().project.chains[0].blocks[0].enabled,
        "block must not be mutated when block is not found"
    );
}

#[test]
fn toggle_block_enabled_non_existent_chain_returns_err_no_mutation() {
    let session = make_session("chain_0", make_core_block("blk_0", true));
    let session_rc = Rc::new(RefCell::new(session));
    let dispatcher = LocalDispatcher {
        project_session: Rc::clone(&session_rc),
    };

    let result = dispatcher.dispatch(Command::ToggleBlockEnabled {
        chain: ChainId("chain_MISSING".to_string()),
        block: BlockId("blk_0".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert!(
        session_rc.borrow().project.chains[0].blocks[0].enabled,
        "block must not be mutated when chain is not found"
    );
}
