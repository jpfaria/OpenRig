//! Tests for `LocalDispatcher` commands.
//!
//! Follows strict TDD: tests were written first (RED), then the implementation
//! was added to `local_dispatcher.rs` (GREEN).
//!
//! This module is also the ROOT of the `ld_*` test family: the shared fixtures
//! below are `pub(super)` and the sibling modules (`ld_block2`, `ld_chain`,
//! `ld_block_param`, …) glob-import them. Its own tests are the
//! `ToggleBlockEnabled` cases; parameter writes live in `ld_block_param_tests.rs`.
//!
//! Attached to `lib.rs` via:
//! ```text
//! #[cfg(test)]
//! #[path = "local_dispatcher_tests.rs"]
//! mod local_dispatcher_tests;
//! ```

pub(super) use std::cell::RefCell;
pub(super) use std::rc::Rc;

pub(super) use domain::ids::{BlockId, ChainId, DeviceId};
pub(super) use domain::value_objects::ParameterValue;
pub(super) use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
pub(super) use project::chain::Chain;
pub(super) use project::param::ParameterSet;
pub(super) use project::project::Project;

pub(super) use crate::command::{
    BlockCommand, ChainCommand, Command, MidiCommand, ProjectCommand, SelectionCommand,
    SettingsCommand,
};
pub(super) use crate::dispatcher::CommandDispatcher;
pub(super) use crate::event::Event;
pub(super) use crate::local_dispatcher::LocalDispatcher;

// ── helpers ──────────────────────────────────────────────────────────────────

pub(super) fn make_core_block(id: &str, enabled: bool) -> AudioBlock {
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

pub(super) fn make_core_block_with_param(id: &str, param_path: &str, value: f32) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert(param_path, ParameterValue::Float(value));
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".to_string(),
            model: "test_model".to_string(),
            params,
        }),
    }
}

pub(super) fn make_core_block_with_bool_param(
    id: &str,
    param_path: &str,
    value: bool,
) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert(param_path, ParameterValue::Bool(value));
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".to_string(),
            model: "test_model".to_string(),
            params,
        }),
    }
}

pub(super) fn make_core_block_with_string_param(
    id: &str,
    param_path: &str,
    value: &str,
) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert(param_path, ParameterValue::String(value.to_string()));
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".to_string(),
            model: "test_model".to_string(),
            params,
        }),
    }
}

pub(super) fn make_project(chain_id: &str, block: AudioBlock) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(chain_id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![block],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }))
}

// ── tests ─────────────────────────────────────────────────────────────────────

pub(crate) fn empty_project_rc() -> std::rc::Rc<std::cell::RefCell<Project>> {
    std::rc::Rc::new(std::cell::RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }))
}

pub(super) use super::ld_chain::{
    make_chain_with_input, make_empty_chain, make_project_three_chains,
};
pub(super) use super::ld_insert::make_device_settings;
pub(super) use super::ld_savechain::{
    make_output_block, make_project_with_input_chain, make_project_with_io_chain,
};

#[test]
fn toggle_block_enabled_flips_true_to_false_and_emits_event() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    // #127: no `RuntimeControl` is attached here, so the dispatcher also reports
    // that the chain's runtime sync is still owed — the frontend's drain turns
    // that into the sync sequence this path used to call directly. With a
    // runtime attached only the first event is emitted.
    assert_eq!(events.len(), 2, "unexpected events: {events:?}");
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
        matches!(
            &events[1],
            Event::ChainRuntimeSyncNeeded { chain } if chain.0 == "chain_0"
        ),
        "unexpected event: {:?}",
        events[1]
    );
    assert!(
        !project.borrow().chains[0].blocks[0].enabled,
        "block should be disabled after toggle"
    );
}

#[test]
fn toggle_block_enabled_non_existent_block_returns_err_no_mutation() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
    }));

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    assert!(
        project.borrow().chains[0].blocks[0].enabled,
        "block must not be mutated when block is not found"
    );
}

#[test]
fn toggle_block_enabled_non_existent_chain_returns_err_no_mutation() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
        chain: ChainId("chain_MISSING".to_string()),
        block: BlockId("blk_0".to_string()),
    }));

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert!(
        project.borrow().chains[0].blocks[0].enabled,
        "block must not be mutated when chain is not found"
    );
}
