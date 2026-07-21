//! Tests for `LocalDispatcher` commands.
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

pub(super) use std::cell::RefCell;
pub(super) use std::rc::Rc;

pub(super) use domain::ids::{BlockId, ChainId, DeviceId};
pub(super) use domain::value_objects::ParameterValue;
pub(super) use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
pub(super) use project::chain::Chain;
pub(super) use project::param::ParameterSet;
pub(super) use project::project::Project;

pub(super) use crate::command::Command;
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

pub(super) fn make_core_block_with_bool_param(id: &str, param_path: &str, value: bool) -> AudioBlock {
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

pub(super) fn make_core_block_with_string_param(id: &str, param_path: &str, value: &str) -> AudioBlock {
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

pub(super) use super::ld_chain::{make_chain_with_input, make_empty_chain, make_project_three_chains};
pub(super) use super::ld_savechain::{make_output_block, make_project_with_input_chain, make_project_with_io_chain};
pub(super) use super::ld_insert::make_device_settings;

#[test]
fn toggle_block_enabled_flips_true_to_false_and_emits_event() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

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
        !project.borrow().chains[0].blocks[0].enabled,
        "block should be disabled after toggle"
    );
}

#[test]
fn toggle_block_enabled_non_existent_block_returns_err_no_mutation() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::ToggleBlockEnabled {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
    });

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

    let result = dispatcher.dispatch(Command::ToggleBlockEnabled {
        chain: ChainId("chain_MISSING".to_string()),
        block: BlockId("blk_0".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert!(
        project.borrow().chains[0].blocks[0].enabled,
        "block must not be mutated when chain is not found"
    );
}

// ── SetBlockParameterNumber tests ─────────────────────────────────────────────

#[test]
fn set_block_parameter_number_writes_value_and_emits_event() {
    let block = make_core_block_with_param("blk_0", "gain", 0.5);
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetBlockParameterNumber {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "gain".to_string(),
        value: 0.8,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(
            &events[0],
            Event::BlockParameterChanged { chain, block, path }
            if chain.0 == "chain_0" && block.0 == "blk_0" && path == "gain"
        ),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    let stored = core.params.get_f32("gain");
    assert!(
        stored.is_some(),
        "gain parameter must be present after write"
    );
    let stored_value = stored.unwrap();
    assert!(
        (stored_value - 0.8_f32).abs() < 1e-5,
        "expected gain ~0.8, got {stored_value}"
    );
}

#[test]
fn set_block_parameter_number_non_existent_block_returns_err() {
    let block = make_core_block_with_param("blk_0", "gain", 0.5);
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetBlockParameterNumber {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
        path: "gain".to_string(),
        value: 0.8,
    });

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    // Original value must be unchanged
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_f32("gain"),
        Some(0.5_f32),
        "gain must not be mutated when block is not found"
    );
}

#[test]
fn set_block_parameter_number_unknown_path_inserts_without_touching_other_params() {
    // Post-#496: `set_parameter_number` no longer rejects unknown paths;
    // a NAM block saved before #496 (when `output_db` was filtered out
    // of the schema) has no entry for it yet, and the dispatch layer
    // only emits paths drawn from the active schema. So writing a new
    // path is a valid insert. What this test still pins is the
    // ISOLATION contract: setting one path must not corrupt the value
    // of another, existing parameter on the same block.
    let block = make_core_block_with_param("blk_0", "gain", 0.5);
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetBlockParameterNumber {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "newly_exposed_param".to_string(),
        value: 0.8,
    });

    assert!(
        result.is_ok(),
        "unknown path must insert (post-#496), got: {:?}",
        result.err()
    );
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_f32("gain"),
        Some(0.5_f32),
        "writing newly_exposed_param must not touch the gain value"
    );
    assert_eq!(
        core.params.get_f32("newly_exposed_param"),
        Some(0.8_f32),
        "the newly-exposed path must now be set to the dispatched value"
    );
}

// ── SetBlockParameterBool tests ───────────────────────────────────────────────

#[test]
fn set_block_parameter_bool_writes_value_and_emits_event() {
    let block = make_core_block_with_bool_param("blk_0", "bypass", false);
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetBlockParameterBool {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "bypass".to_string(),
        value: true,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(
            &events[0],
            Event::BlockParameterChanged { chain, block, path }
            if chain.0 == "chain_0" && block.0 == "blk_0" && path == "bypass"
        ),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_bool("bypass"),
        Some(true),
        "bypass must be true after write"
    );
}

#[test]
fn set_block_parameter_bool_non_existent_block_returns_err() {
    let block = make_core_block_with_bool_param("blk_0", "bypass", false);
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetBlockParameterBool {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
        path: "bypass".to_string(),
        value: true,
    });

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_bool("bypass"),
        Some(false),
        "bypass must not be mutated when block is not found"
    );
}

#[test]
fn set_block_parameter_bool_non_existent_path_returns_err() {
    let block = make_core_block_with_bool_param("blk_0", "bypass", false);
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetBlockParameterBool {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "no_such_param".to_string(),
        value: true,
    });

    assert!(result.is_err(), "expected Err for missing path, got Ok");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no_such_param"),
        "error message must mention the missing path, got: {err_msg}"
    );
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_bool("bypass"),
        Some(false),
        "bypass must not be mutated when path is not found"
    );
}

// ── SetBlockParameterText tests ───────────────────────────────────────────────

#[test]
fn set_block_parameter_text_writes_value_and_emits_event() {
    let block = make_core_block_with_string_param("blk_0", "label", "old");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetBlockParameterText {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "label".to_string(),
        value: "new_value".to_string(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(
            &events[0],
            Event::BlockParameterChanged { chain, block, path }
            if chain.0 == "chain_0" && block.0 == "blk_0" && path == "label"
        ),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_string("label"),
        Some("new_value"),
        "label must be 'new_value' after write"
    );
}

#[test]
fn set_block_parameter_text_non_existent_block_returns_err() {
    let block = make_core_block_with_string_param("blk_0", "label", "old");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetBlockParameterText {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
        path: "label".to_string(),
        value: "new_value".to_string(),
    });

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_string("label"),
        Some("old"),
        "label must not be mutated when block is not found"
    );
}

#[test]
fn set_block_parameter_text_non_existent_path_returns_err() {
    let block = make_core_block_with_string_param("blk_0", "label", "old");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetBlockParameterText {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "no_such_param".to_string(),
        value: "new_value".to_string(),
    });

    assert!(result.is_err(), "expected Err for missing path, got Ok");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no_such_param"),
        "error message must mention the missing path, got: {err_msg}"
    );
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_string("label"),
        Some("old"),
        "label must not be mutated when path is not found"
    );
}

// ── SelectBlockParameterOption tests ─────────────────────────────────────────

#[test]
fn select_block_parameter_option_writes_value_and_emits_event() {
    let block = make_core_block_with_string_param("blk_0", "mode", "option_a");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SelectBlockParameterOption {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "mode".to_string(),
        value: "option_b".to_string(),
        index: 1,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(
            &events[0],
            Event::BlockParameterChanged { chain, block, path }
            if chain.0 == "chain_0" && block.0 == "blk_0" && path == "mode"
        ),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_string("mode"),
        Some("option_b"),
        "mode must be 'option_b' after write"
    );
}

#[test]
fn select_block_parameter_option_non_existent_block_returns_err() {
    let block = make_core_block_with_string_param("blk_0", "mode", "option_a");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SelectBlockParameterOption {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
        path: "mode".to_string(),
        value: "option_b".to_string(),
        index: 1,
    });

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_string("mode"),
        Some("option_a"),
        "mode must not be mutated when block is not found"
    );
}

#[test]
fn select_block_parameter_option_non_existent_path_returns_err() {
    let block = make_core_block_with_string_param("blk_0", "mode", "option_a");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SelectBlockParameterOption {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "no_such_param".to_string(),
        value: "option_b".to_string(),
        index: 1,
    });

    assert!(result.is_err(), "expected Err for missing path, got Ok");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no_such_param"),
        "error message must mention the missing path, got: {err_msg}"
    );
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_string("mode"),
        Some("option_a"),
        "mode must not be mutated when path is not found"
    );
}

