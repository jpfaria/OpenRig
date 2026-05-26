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

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::value_objects::ParameterValue;
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use project::project::Project;

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

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

fn make_core_block_with_param(id: &str, param_path: &str, value: f32) -> AudioBlock {
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

fn make_core_block_with_bool_param(id: &str, param_path: &str, value: bool) -> AudioBlock {
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

fn make_core_block_with_string_param(id: &str, param_path: &str, value: &str) -> AudioBlock {
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

fn make_project(chain_id: &str, block: AudioBlock) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(chain_id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            blocks: vec![block],
        }],
        midi: None,
    }))
}

// ── tests ─────────────────────────────────────────────────────────────────────

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
fn set_block_parameter_number_non_existent_path_returns_err() {
    let block = make_core_block_with_param("blk_0", "gain", 0.5);
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetBlockParameterNumber {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "no_such_param".to_string(),
        value: 0.8,
    });

    assert!(result.is_err(), "expected Err for missing path, got Ok");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no_such_param"),
        "error message must mention the missing path, got: {err_msg}"
    );
    // Original value must be unchanged
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_f32("gain"),
        Some(0.5_f32),
        "gain must not be mutated when path is not found"
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

// ── PickBlockParameterFile tests ──────────────────────────────────────────────

#[test]
fn pick_block_parameter_file_writes_path_and_emits_event() {
    let block = make_core_block_with_string_param("blk_0", "ir_path", "/old/path.wav");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    use std::path::PathBuf;
    let result = dispatcher.dispatch(Command::PickBlockParameterFile {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "ir_path".to_string(),
        file: PathBuf::from("/new/file.wav"),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(
            &events[0],
            Event::BlockParameterChanged { chain, block, path }
            if chain.0 == "chain_0" && block.0 == "blk_0" && path == "ir_path"
        ),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_string("ir_path"),
        Some("/new/file.wav"),
        "ir_path must be updated to new file path"
    );
}

#[test]
fn pick_block_parameter_file_non_existent_block_returns_err() {
    use std::path::PathBuf;
    let block = make_core_block_with_string_param("blk_0", "ir_path", "/old/path.wav");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::PickBlockParameterFile {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
        path: "ir_path".to_string(),
        file: PathBuf::from("/new/file.wav"),
    });

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_string("ir_path"),
        Some("/old/path.wav"),
        "ir_path must not be mutated when block is not found"
    );
}

#[test]
fn pick_block_parameter_file_non_existent_path_returns_err() {
    use std::path::PathBuf;
    let block = make_core_block_with_string_param("blk_0", "ir_path", "/old/path.wav");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::PickBlockParameterFile {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "no_such_param".to_string(),
        file: PathBuf::from("/new/file.wav"),
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
        core.params.get_string("ir_path"),
        Some("/old/path.wav"),
        "ir_path must not be mutated when path is not found"
    );
}

// ── RemoveBlock tests ─────────────────────────────────────────────────────────

fn make_project_two_blocks(chain_id: &str) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(chain_id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            blocks: vec![
                make_core_block("blk_0", true),
                make_core_block("blk_1", true),
            ],
        }],
        midi: None,
    }))
}

#[test]
fn remove_block_removes_block_and_emits_event() {
    let project = make_project_two_blocks("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::RemoveBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(
            &events[0],
            Event::BlockRemoved { chain, block }
            if chain.0 == "chain_0" && block.0 == "blk_0"
        ),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    assert_eq!(
        proj.chains[0].blocks.len(),
        1,
        "chain must have 1 block after remove"
    );
    assert_eq!(
        proj.chains[0].blocks[0].id.0, "blk_1",
        "remaining block must be blk_1"
    );
}

#[test]
fn remove_block_non_existent_block_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::RemoveBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    assert_eq!(
        project.borrow().chains[0].blocks.len(),
        1,
        "block list must not change when block is not found"
    );
}

#[test]
fn remove_block_non_existent_chain_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::RemoveBlock {
        chain: ChainId("chain_MISSING".to_string()),
        block: BlockId("blk_0".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert_eq!(
        project.borrow().chains[0].blocks.len(),
        1,
        "block list must not change when chain is not found"
    );
}

// ── MoveBlock tests ───────────────────────────────────────────────────────────

fn make_project_three_blocks(chain_id: &str) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(chain_id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            blocks: vec![
                make_core_block("blk_0", true),
                make_core_block("blk_1", true),
                make_core_block("blk_2", true),
            ],
        }],
        midi: None,
    }))
}

#[test]
fn move_block_reorders_blocks_and_emits_event() {
    // Move blk_2 to position 0 → order becomes [blk_2, blk_0, blk_1]
    let project = make_project_three_blocks("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::MoveBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_2".to_string()),
        new_position: 0,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], Event::ChainReloaded { chain } if chain.0 == "chain_0"),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains[0]
        .blocks
        .iter()
        .map(|b| b.id.0.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["blk_2", "blk_0", "blk_1"],
        "blocks must be reordered"
    );
}

#[test]
fn move_block_past_end_clamps_to_end() {
    // Move blk_0 to position 999 → it should end up at the end: [blk_1, blk_2, blk_0]
    let project = make_project_three_blocks("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::MoveBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        new_position: 999,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains[0]
        .blocks
        .iter()
        .map(|b| b.id.0.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["blk_1", "blk_2", "blk_0"],
        "block must be clamped to end"
    );
}

#[test]
fn move_block_non_existent_block_returns_err() {
    let project = make_project_three_blocks("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::MoveBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
        new_position: 0,
    });

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains[0]
        .blocks
        .iter()
        .map(|b| b.id.0.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["blk_0", "blk_1", "blk_2"],
        "order must not change when block not found"
    );
}

// ── AddBlock tests ────────────────────────────────────────────────────────────

#[test]
fn add_block_inserts_block_and_emits_event() {
    // chain_0 has one block; add a gain block at position 0
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::AddBlock {
        chain: ChainId("chain_0".to_string()),
        kind: "gain".to_string(),
        model_id: "fuzz_ge".to_string(),
        position: 0,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], Event::BlockAdded { chain, block: _ } if chain.0 == "chain_0"),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    assert_eq!(
        proj.chains[0].blocks.len(),
        2,
        "chain must have 2 blocks after add"
    );
    // The newly added block is at position 0
    let new_block = &proj.chains[0].blocks[0];
    assert_ne!(new_block.id.0, "blk_0", "inserted block must have a new id");
    assert!(new_block.enabled, "new block must be enabled by default");
}

#[test]
fn add_block_past_end_clamps_to_end() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::AddBlock {
        chain: ChainId("chain_0".to_string()),
        kind: "gain".to_string(),
        model_id: "fuzz_ge".to_string(),
        position: 999,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    assert_eq!(
        proj.chains[0].blocks.len(),
        2,
        "chain must have 2 blocks after add"
    );
    // The original block is still at position 0; new block is at the end
    assert_eq!(
        proj.chains[0].blocks[0].id.0, "blk_0",
        "original block stays first"
    );
}

#[test]
fn add_block_non_existent_chain_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::AddBlock {
        chain: ChainId("chain_MISSING".to_string()),
        kind: "gain".to_string(),
        model_id: "fuzz_ge".to_string(),
        position: 0,
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert_eq!(
        project.borrow().chains[0].blocks.len(),
        1,
        "block list must not change when chain not found"
    );
}

#[test]
fn add_block_unknown_model_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::AddBlock {
        chain: ChainId("chain_0".to_string()),
        kind: "gain".to_string(),
        model_id: "no_such_model".to_string(),
        position: 0,
    });

    assert!(result.is_err(), "expected Err for unknown model, got Ok");
    assert_eq!(
        project.borrow().chains[0].blocks.len(),
        1,
        "block list must not change when model is unknown"
    );
}

// ── ReplaceBlockModel tests ───────────────────────────────────────────────────

#[test]
fn replace_block_model_swaps_kind_preserves_id_enabled_and_emits_event() {
    let project = make_project("chain_0", make_core_block("blk_0", false));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::ReplaceBlockModel {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        model_id: "fuzz_ge".to_string(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(
            &events[0],
            Event::BlockReplaced { chain, block }
            if chain.0 == "chain_0" && block.0 == "blk_0"
        ),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    let block = &proj.chains[0].blocks[0];
    // id and enabled must be preserved
    assert_eq!(
        block.id.0, "blk_0",
        "block id must be preserved after model swap"
    );
    assert!(
        !block.enabled,
        "block enabled state must be preserved after model swap"
    );
    // kind must have changed — it should now be a gain/fuzz_ge block
    assert!(
        matches!(&block.kind, AudioBlockKind::Core(cb) if cb.model == "fuzz_ge"),
        "block kind must be fuzz_ge after replace, got {:?}",
        block.kind.label()
    );
}

#[test]
fn replace_block_model_non_existent_block_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::ReplaceBlockModel {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
        model_id: "fuzz_ge".to_string(),
    });

    assert!(result.is_err(), "expected Err for missing block, got Ok");
}

#[test]
fn replace_block_model_unknown_model_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::ReplaceBlockModel {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        model_id: "no_such_model_xyz".to_string(),
    });

    assert!(result.is_err(), "expected Err for unknown model, got Ok");
    // Kind must not be mutated
    let proj = project.borrow();
    assert!(
        matches!(&proj.chains[0].blocks[0].kind, AudioBlockKind::Core(cb) if cb.model == "test_model"),
        "block kind must not change on error"
    );
}

/// Issue #537 — positive contract pinned with native cabs: swapping one
/// native cab model for another must keep the slot's `effect_type` as
/// `"cab"`. The reported regression is on disk-package IR cabs (see the
/// integration test in `tests/issue_537_replace_block_model_disk_package_cab.rs`);
/// this unit test guards the parallel native code path so we know if the
/// fix accidentally damages native swaps.
#[test]
fn replace_block_model_keeps_cab_effect_type_when_swapping_two_native_cabs() {
    let block = AudioBlock {
        id: BlockId("blk_cab".to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "cab".to_string(),
            model: "american_2x12".to_string(),
            params: ParameterSet::default(),
        }),
    };
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::ReplaceBlockModel {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_cab".to_string()),
        model_id: "brit_4x12".to_string(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    let block = &proj.chains[0].blocks[0];
    let core = match &block.kind {
        AudioBlockKind::Core(cb) => cb,
        other => panic!(
            "expected Core variant after cab->cab native swap, got variant '{}'",
            other.label()
        ),
    };
    assert_eq!(
        core.effect_type, "cab",
        "effect_type must stay 'cab' after swapping one native cab for another \
         (got '{}' on slot now hosting '{}')",
        core.effect_type, core.model
    );
    assert_eq!(
        core.model, "brit_4x12",
        "model must be the newly picked 'brit_4x12'"
    );
}

// ── Chain-level test helpers ──────────────────────────────────────────────────

/// Build a chain with an InputBlock on device `dev_id`, channel `ch`.
fn make_chain_with_input(chain_id: &str, dev_id: &str, ch: usize, enabled: bool) -> Chain {
    Chain {
        id: ChainId(chain_id.to_string()),
        description: Some(chain_id.to_string()),
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        blocks: vec![AudioBlock {
            id: BlockId("input:0".to_string()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: vec![InputEntry {
                    device_id: DeviceId(dev_id.to_string()),
                    mode: ChainInputMode::Mono,
                    channels: vec![ch],
                }],
            }),
        }],
    }
}

/// Build a minimal chain with no blocks.
fn make_empty_chain(chain_id: &str, enabled: bool) -> Chain {
    Chain {
        id: ChainId(chain_id.to_string()),
        description: Some(chain_id.to_string()),
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        blocks: vec![],
    }
}

/// Project with two chains: chain_0 (enabled, dev_a ch 0) and chain_1 (disabled, dev_a ch 0).
fn make_project_two_chains() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![
            make_chain_with_input("chain_0", "dev_a", 0, true),
            make_chain_with_input("chain_1", "dev_a", 0, false),
        ],
        midi: None,
    }))
}

/// Project with three chains in order: chain_a, chain_b, chain_c.
fn make_project_three_chains() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![
            make_empty_chain("chain_a", false),
            make_empty_chain("chain_b", false),
            make_empty_chain("chain_c", false),
        ],
        midi: None,
    }))
}

// ── RemoveChain tests ─────────────────────────────────────────────────────────

#[test]
fn remove_chain_removes_chain_and_emits_event() {
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::RemoveChain {
        chain: ChainId("chain_0".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    // Should emit ChainRemoved + ProjectMutated.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainRemoved { chain } if chain.0 == "chain_0")),
        "expected ChainRemoved event, got {:?}",
        events
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated event"
    );
    let proj = project.borrow();
    assert_eq!(proj.chains.len(), 1, "one chain must remain");
    assert_eq!(proj.chains[0].id.0, "chain_1", "chain_1 must remain");
}

#[test]
fn remove_chain_non_existent_returns_err_no_mutation() {
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::RemoveChain {
        chain: ChainId("chain_MISSING".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert_eq!(
        project.borrow().chains.len(),
        2,
        "chain list must not change when chain not found"
    );
}

// ── MoveChainUp tests ─────────────────────────────────────────────────────────

#[test]
fn move_chain_up_reorders_and_emits_event() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Move chain_b (index 1) up → should become index 0.
    let result = dispatcher.dispatch(Command::MoveChainUp {
        chain: ChainId("chain_b".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainMoved { chain, new_position: 0 }
            if chain.0 == "chain_b"
        )),
        "expected ChainMoved{{chain_b, 0}}, got {:?}",
        events
    );
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains.iter().map(|c| c.id.0.as_str()).collect();
    assert_eq!(ids, vec!["chain_b", "chain_a", "chain_c"]);
}

#[test]
fn move_chain_up_at_index_zero_returns_ok_no_op() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // chain_a is at index 0 — already at top, should be a no-op.
    let result = dispatcher.dispatch(Command::MoveChainUp {
        chain: ChainId("chain_a".to_string()),
    });

    assert!(result.is_ok(), "no-op should return Ok");
    let events = result.unwrap();
    assert!(events.is_empty(), "no-op must produce no events");
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains.iter().map(|c| c.id.0.as_str()).collect();
    assert_eq!(
        ids,
        vec!["chain_a", "chain_b", "chain_c"],
        "order must not change"
    );
}

#[test]
fn move_chain_up_non_existent_returns_err() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::MoveChainUp {
        chain: ChainId("chain_MISSING".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

// ── MoveChainDown tests ───────────────────────────────────────────────────────

#[test]
fn move_chain_down_reorders_and_emits_event() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Move chain_b (index 1) down → should become index 2.
    let result = dispatcher.dispatch(Command::MoveChainDown {
        chain: ChainId("chain_b".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainMoved { chain, new_position: 2 }
            if chain.0 == "chain_b"
        )),
        "expected ChainMoved{{chain_b, 2}}, got {:?}",
        events
    );
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains.iter().map(|c| c.id.0.as_str()).collect();
    assert_eq!(ids, vec!["chain_a", "chain_c", "chain_b"]);
}

#[test]
fn move_chain_down_at_last_index_returns_ok_no_op() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // chain_c is at the last index — should be a no-op.
    let result = dispatcher.dispatch(Command::MoveChainDown {
        chain: ChainId("chain_c".to_string()),
    });

    assert!(result.is_ok(), "no-op should return Ok");
    let events = result.unwrap();
    assert!(events.is_empty(), "no-op must produce no events");
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains.iter().map(|c| c.id.0.as_str()).collect();
    assert_eq!(
        ids,
        vec!["chain_a", "chain_b", "chain_c"],
        "order must not change"
    );
}

#[test]
fn move_chain_down_non_existent_returns_err() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::MoveChainDown {
        chain: ChainId("chain_MISSING".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

// ── ToggleChainEnabled tests ──────────────────────────────────────────────────

#[test]
fn toggle_chain_enabled_enables_disabled_chain() {
    // chain_1 uses dev_a ch 0, and chain_0 also uses dev_a ch 0 (and is enabled).
    // But chain_1 shares the channel — expect conflict.
    // First test a clean enable: use a project with no conflict.
    let project_no_conflict = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![
            make_chain_with_input("chain_0", "dev_a", 0, true),
            make_chain_with_input("chain_1", "dev_b", 0, false), // different device
        ],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project_no_conflict));

    let result = dispatcher.dispatch(Command::ToggleChainEnabled {
        chain: ChainId("chain_1".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainEnabledChanged { chain, enabled: true }
            if chain.0 == "chain_1"
        )),
        "expected ChainEnabledChanged{{chain_1, true}}, got {:?}",
        events
    );
    assert!(
        project_no_conflict.borrow().chains[1].enabled,
        "chain_1 must be enabled after toggle"
    );
}

#[test]
fn toggle_chain_enabled_disables_enabled_chain() {
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Toggle chain_0 (currently enabled) → should disable it.
    let result = dispatcher.dispatch(Command::ToggleChainEnabled {
        chain: ChainId("chain_0".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainEnabledChanged { chain, enabled: false }
            if chain.0 == "chain_0"
        )),
        "expected ChainEnabledChanged{{chain_0, false}}, got {:?}",
        events
    );
    assert!(
        !project.borrow().chains[0].enabled,
        "chain_0 must be disabled after toggle"
    );
}

#[test]
fn toggle_chain_enabled_conflict_returns_err() {
    // chain_0 (enabled, dev_a ch 0), chain_1 (disabled, dev_a ch 0).
    // Enabling chain_1 should fail because chain_0 already uses dev_a ch 0.
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::ToggleChainEnabled {
        chain: ChainId("chain_1".to_string()),
    });

    assert!(result.is_err(), "expected Err for channel conflict, got Ok");
    // chain_1 must remain disabled.
    assert!(
        !project.borrow().chains[1].enabled,
        "chain_1 must remain disabled after conflict error"
    );
}

#[test]
fn toggle_chain_enabled_non_existent_returns_err() {
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::ToggleChainEnabled {
        chain: ChainId("chain_MISSING".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

// ── AddChain tests ────────────────────────────────────────────────────────────

#[test]
fn add_chain_appends_chain_and_emits_event() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_chain = make_empty_chain("chain_new", false);
    let new_id = new_chain.id.clone();

    let result = dispatcher.dispatch(Command::AddChain { chain: new_chain });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainAdded { chain } if *chain == new_id)),
        "expected ChainAdded event, got {:?}",
        events
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated event"
    );
    assert_eq!(
        project.borrow().chains.len(),
        4,
        "project must have 4 chains after add"
    );
    assert_eq!(
        project.borrow().chains.last().unwrap().id,
        new_id,
        "new chain must be last"
    );
}

#[test]
fn add_chain_enabled_true_with_conflict_returns_err() {
    // chain_0 is enabled on dev_a ch 0. Add a new chain (enabled=true) on dev_a ch 0 → conflict.
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let mut conflicting_chain = make_chain_with_input("chain_new", "dev_a", 0, true); // enabled=true!
    conflicting_chain.enabled = true;

    let result = dispatcher.dispatch(Command::AddChain {
        chain: conflicting_chain,
    });

    assert!(result.is_err(), "expected Err for channel conflict, got Ok");
    assert_eq!(
        project.borrow().chains.len(),
        2,
        "project must still have 2 chains after error"
    );
}

#[test]
fn add_chain_enabled_false_no_conflict_check() {
    // chain_0 is enabled on dev_a ch 0. Add a new chain (enabled=false) on same channel → ok.
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_chain = make_chain_with_input("chain_new", "dev_a", 0, false); // enabled=false

    let result = dispatcher.dispatch(Command::AddChain { chain: new_chain });

    assert!(
        result.is_ok(),
        "disabled chain add must succeed even with same channel"
    );
    assert_eq!(project.borrow().chains.len(), 3);
}

// ── ConfigureChain tests ──────────────────────────────────────────────────────

#[test]
fn configure_chain_updates_metadata_and_emits_event() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Get chain_b's id, build an updated version.
    let chain_b_id = project.borrow().chains[1].id.clone();
    let mut updated = project.borrow().chains[1].clone();
    updated.description = Some("Updated Name".to_string());
    updated.instrument = "bass".to_string();

    let result = dispatcher.dispatch(Command::ConfigureChain { chain: updated });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainConfigured { chain } if *chain == chain_b_id)),
        "expected ChainConfigured event, got {:?}",
        events
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated event"
    );
    let proj = project.borrow();
    let chain_b = proj.chains.iter().find(|c| c.id == chain_b_id).unwrap();
    assert_eq!(chain_b.description.as_deref(), Some("Updated Name"));
    assert_eq!(chain_b.instrument, "bass");
}

#[test]
fn configure_chain_preserves_enabled_state() {
    // chain_a starts enabled=false. Even if the supplied Chain has enabled=true,
    // the dispatcher must preserve the original enabled state.
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let chain_a_id = project.borrow().chains[0].id.clone();
    let mut updated = project.borrow().chains[0].clone();
    updated.enabled = true; // caller tries to sneak in enabled=true via ConfigureChain

    let result = dispatcher.dispatch(Command::ConfigureChain { chain: updated });

    assert!(result.is_ok());
    let proj = project.borrow();
    let chain_a = proj.chains.iter().find(|c| c.id == chain_a_id).unwrap();
    assert!(
        !chain_a.enabled,
        "enabled state must be preserved (use ToggleChainEnabled to change it)"
    );
}

#[test]
fn configure_chain_non_existent_returns_err() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let mut ghost = make_empty_chain("chain_ghost", false);
    ghost.id = ChainId("chain_MISSING".to_string());

    let result = dispatcher.dispatch(Command::ConfigureChain { chain: ghost });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert_eq!(
        project.borrow().chains.len(),
        3,
        "chain list must not change"
    );
}

// ── SaveChain tests ───────────────────────────────────────────────────────────

#[test]
fn save_chain_replaces_existing_and_emits_event() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let chain_b_id = project.borrow().chains[1].id.clone();
    let mut updated = project.borrow().chains[1].clone();
    updated.description = Some("Chain B Updated".to_string());
    updated.instrument = "bass".to_string();

    let result = dispatcher.dispatch(Command::SaveChain { chain: updated });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainSaved { chain } if *chain == chain_b_id)),
        "expected ChainSaved event, got {:?}",
        events
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated"
    );
    let proj = project.borrow();
    let chain_b = proj.chains.iter().find(|c| c.id == chain_b_id).unwrap();
    assert_eq!(chain_b.description.as_deref(), Some("Chain B Updated"));
    assert_eq!(chain_b.instrument, "bass");
    // enabled must be preserved
    assert!(!chain_b.enabled, "enabled must be preserved by SaveChain");
}

#[test]
fn save_chain_appends_when_id_not_found() {
    // SaveChain with an unknown id should append (create flow).
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_chain = make_empty_chain("chain_brand_new", false);

    let result = dispatcher.dispatch(Command::SaveChain { chain: new_chain });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    assert_eq!(
        project.borrow().chains.len(),
        4,
        "chain must be appended when id not found"
    );
    assert_eq!(
        project.borrow().chains.last().unwrap().id.0,
        "chain_brand_new"
    );
}

#[test]
fn save_chain_preserves_enabled_state() {
    // Even if caller sends enabled=true in the chain, SaveChain must keep the
    // existing enabled state (same rule as ConfigureChain).
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let chain_a_id = project.borrow().chains[0].id.clone();
    let mut updated = project.borrow().chains[0].clone();
    updated.enabled = true; // sneaking in enabled=true

    let result = dispatcher.dispatch(Command::SaveChain { chain: updated });

    assert!(result.is_ok());
    let proj = project.borrow();
    let chain_a = proj.chains.iter().find(|c| c.id == chain_a_id).unwrap();
    assert!(!chain_a.enabled, "SaveChain must preserve enabled state");
}

// ── SaveChainInputEndpoints tests ─────────────────────────────────────────────

fn make_project_with_input_chain() -> (Rc<RefCell<Project>>, ChainId) {
    let chain = make_chain_with_input("chain_io", "dev_x", 0, false);
    let chain_id = chain.id.clone();
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain],
        midi: None,
    }));
    (project, chain_id)
}

fn make_input_block(dev_id: &str, ch: usize) -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".to_string()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            entries: vec![InputEntry {
                device_id: DeviceId(dev_id.to_string()),
                mode: ChainInputMode::Mono,
                channels: vec![ch],
            }],
        }),
    }
}

fn make_output_block(dev_id: &str, ch: usize) -> AudioBlock {
    AudioBlock {
        id: BlockId("output:0".to_string()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            entries: vec![OutputEntry {
                device_id: DeviceId(dev_id.to_string()),
                mode: ChainOutputMode::Stereo,
                channels: vec![ch],
            }],
        }),
    }
}

#[test]
fn save_chain_input_endpoints_replaces_input_block_and_emits_event() {
    let (project, chain_id) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_input = make_input_block("dev_new", 3);

    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: chain_id.clone(),
        input_blocks: vec![new_input],
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainInputEndpointsSaved { chain } if *chain == chain_id)),
        "expected ChainInputEndpointsSaved, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let input_blocks: Vec<_> = chain
        .blocks
        .iter()
        .filter(|b| matches!(&b.kind, AudioBlockKind::Input(_)))
        .collect();
    assert_eq!(
        input_blocks.len(),
        1,
        "chain must have exactly one input block"
    );
    if let AudioBlockKind::Input(ib) = &input_blocks[0].kind {
        assert_eq!(ib.entries[0].device_id.0, "dev_new");
        assert_eq!(ib.entries[0].channels, vec![3]);
    }
}

#[test]
fn save_chain_input_endpoints_multi_block_replaces_all_and_emits_event() {
    // Build a chain with one existing input block + one core block.
    let chain_id = ChainId("chain_multi".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            blocks: vec![
                make_input_block("dev_old", 0),
                make_core_block("blk_mid", true),
            ],
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_inputs = vec![make_input_block("dev_a", 1), make_input_block("dev_b", 2)];

    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: chain_id.clone(),
        input_blocks: new_inputs,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainInputEndpointsSaved { chain } if *chain == chain_id)),
        "expected ChainInputEndpointsSaved"
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    // Two input blocks at head
    let input_count = chain
        .blocks
        .iter()
        .filter(|b| matches!(&b.kind, AudioBlockKind::Input(_)))
        .count();
    assert_eq!(input_count, 2, "chain must have exactly two input blocks");
    // Non-input block is preserved
    assert!(
        chain
            .blocks
            .iter()
            .any(|b| b.id == BlockId("blk_mid".to_string())),
        "non-input block must be preserved"
    );
    // Inputs are at head (first two positions)
    assert!(matches!(&chain.blocks[0].kind, AudioBlockKind::Input(_)));
    assert!(matches!(&chain.blocks[1].kind, AudioBlockKind::Input(_)));
}

#[test]
fn save_chain_input_endpoints_zero_blocks_clears_all_inputs() {
    let (project, chain_id) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: chain_id.clone(),
        input_blocks: vec![],
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let input_count = chain
        .blocks
        .iter()
        .filter(|b| matches!(&b.kind, AudioBlockKind::Input(_)))
        .count();
    assert_eq!(input_count, 0, "all input blocks must be cleared");
}

#[test]
fn save_chain_input_endpoints_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: ChainId("chain_MISSING".to_string()),
        input_blocks: vec![make_input_block("dev_new", 0)],
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

#[test]
fn save_chain_input_endpoints_preserves_non_input_block_order() {
    // Chain: [Input, CoreA, CoreB, Output] → replace Input with two new inputs
    let chain_id = ChainId("chain_order".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            blocks: vec![
                make_input_block("dev_old", 0),
                make_core_block("blk_a", true),
                make_core_block("blk_b", true),
                make_output_block("dev_out", 1),
            ],
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: chain_id.clone(),
        input_blocks: vec![
            make_input_block("dev_new1", 1),
            make_input_block("dev_new2", 2),
        ],
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    // First two are input blocks
    assert!(matches!(&chain.blocks[0].kind, AudioBlockKind::Input(_)));
    assert!(matches!(&chain.blocks[1].kind, AudioBlockKind::Input(_)));
    // Non-input blocks come after, in original relative order
    assert_eq!(chain.blocks[2].id, BlockId("blk_a".to_string()));
    assert_eq!(chain.blocks[3].id, BlockId("blk_b".to_string()));
    // Output block preserved at tail
    assert!(matches!(&chain.blocks[4].kind, AudioBlockKind::Output(_)));
}

// ── SaveChainOutputEndpoints tests ────────────────────────────────────────────

fn make_project_with_io_chain() -> (Rc<RefCell<Project>>, ChainId) {
    let chain_id = ChainId("chain_io_full".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            blocks: vec![make_input_block("dev_a", 0), make_output_block("dev_b", 1)],
        }],
        midi: None,
    }));
    (project, chain_id)
}

#[test]
fn save_chain_output_endpoints_replaces_output_block_and_emits_event() {
    let (project, chain_id) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_output = make_output_block("dev_new_out", 5);

    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: chain_id.clone(),
        output_blocks: vec![new_output],
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainOutputEndpointsSaved { chain } if *chain == chain_id
        )),
        "expected ChainOutputEndpointsSaved, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let out_blocks: Vec<_> = chain
        .blocks
        .iter()
        .filter(|b| matches!(&b.kind, AudioBlockKind::Output(_)))
        .collect();
    assert_eq!(
        out_blocks.len(),
        1,
        "chain must have exactly one output block"
    );
    if let AudioBlockKind::Output(ob) = &out_blocks[0].kind {
        assert_eq!(ob.entries[0].device_id.0, "dev_new_out");
        assert_eq!(ob.entries[0].channels, vec![5]);
    }
}

#[test]
fn save_chain_output_endpoints_multi_block_replaces_all_and_emits_event() {
    // Chain with input + core + output; replace output with two new outputs.
    let chain_id = ChainId("chain_multi_out".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            blocks: vec![
                make_input_block("dev_in", 0),
                make_core_block("blk_mid", true),
                make_output_block("dev_out_old", 1),
            ],
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_outputs = vec![
        make_output_block("dev_out_a", 2),
        make_output_block("dev_out_b", 3),
    ];

    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: chain_id.clone(),
        output_blocks: new_outputs,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainOutputEndpointsSaved { chain } if *chain == chain_id
        )),
        "expected ChainOutputEndpointsSaved"
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let out_count = chain
        .blocks
        .iter()
        .filter(|b| matches!(&b.kind, AudioBlockKind::Output(_)))
        .count();
    assert_eq!(out_count, 2, "chain must have exactly two output blocks");
    // Non-output blocks preserved
    assert!(chain
        .blocks
        .iter()
        .any(|b| matches!(&b.kind, AudioBlockKind::Input(_))));
    assert!(chain
        .blocks
        .iter()
        .any(|b| b.id == BlockId("blk_mid".to_string())));
}

#[test]
fn save_chain_output_endpoints_zero_blocks_clears_all_outputs() {
    let (project, chain_id) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: chain_id.clone(),
        output_blocks: vec![],
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let out_count = chain
        .blocks
        .iter()
        .filter(|b| matches!(&b.kind, AudioBlockKind::Output(_)))
        .count();
    assert_eq!(out_count, 0, "all output blocks must be cleared");
}

#[test]
fn save_chain_output_endpoints_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: ChainId("chain_MISSING".to_string()),
        output_blocks: vec![make_output_block("dev_new_out", 0)],
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

#[test]
fn save_chain_output_endpoints_preserves_non_output_block_order() {
    // Chain: [Input, CoreA, CoreB, Output] → replace Output with two new outputs
    let chain_id = ChainId("chain_out_order".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            blocks: vec![
                make_input_block("dev_in", 0),
                make_core_block("blk_a", true),
                make_core_block("blk_b", true),
                make_output_block("dev_out_old", 1),
            ],
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: chain_id.clone(),
        output_blocks: vec![
            make_output_block("dev_out1", 2),
            make_output_block("dev_out2", 3),
        ],
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    // Non-output blocks come before outputs
    assert!(matches!(&chain.blocks[0].kind, AudioBlockKind::Input(_)));
    assert_eq!(chain.blocks[1].id, BlockId("blk_a".to_string()));
    assert_eq!(chain.blocks[2].id, BlockId("blk_b".to_string()));
    // Two output blocks appended at tail
    assert!(matches!(&chain.blocks[3].kind, AudioBlockKind::Output(_)));
    assert!(matches!(&chain.blocks[4].kind, AudioBlockKind::Output(_)));
}

// ── InsertPrebuiltBlock tests ─────────────────────────────────────────────────

#[test]
fn insert_prebuilt_block_adds_block_at_position_and_emits_event() {
    let (project, chain_id) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_block = make_core_block("blk_new", true);
    let new_block_id = new_block.id.clone();

    let result = dispatcher.dispatch(Command::InsertPrebuiltBlock {
        chain: chain_id.clone(),
        block: new_block,
        position: 0,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(e, Event::BlockAdded { chain, block } if *chain == chain_id && *block == new_block_id)),
        "expected BlockAdded event"
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    assert_eq!(
        chain.blocks[0].id, new_block_id,
        "block must be at position 0"
    );
}

#[test]
fn insert_prebuilt_block_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::InsertPrebuiltBlock {
        chain: ChainId("chain_MISSING".to_string()),
        block: make_core_block("blk_x", true),
        position: 0,
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

#[test]
fn insert_prebuilt_block_position_clamps_to_len() {
    let (project, chain_id) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_block = make_core_block("blk_tail", true);
    let result = dispatcher.dispatch(Command::InsertPrebuiltBlock {
        chain: chain_id.clone(),
        block: new_block,
        position: 9999,
    });

    assert!(result.is_ok());
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    assert_eq!(
        chain.blocks.last().unwrap().id,
        BlockId("blk_tail".to_string()),
        "block must be appended at tail when position exceeds len"
    );
}

// ── OverwriteBlock tests ──────────────────────────────────────────────────────

#[test]
fn overwrite_block_replaces_kind_preserves_id_and_emits_event() {
    let (project, chain_id) = make_project_with_input_chain();
    // Add a core block to edit
    let core_block = make_core_block("blk_edit", true);
    let block_id = core_block.id.clone();
    project
        .borrow_mut()
        .chains
        .iter_mut()
        .find(|c| c.id == chain_id)
        .unwrap()
        .blocks
        .push(core_block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Overwrite with a disabled block
    let mut replacement = make_core_block("blk_replacement_id_ignored", false);
    replacement.enabled = false;
    let result = dispatcher.dispatch(Command::OverwriteBlock {
        chain: chain_id.clone(),
        block: block_id.clone(),
        replacement,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(e, Event::BlockReplaced { chain, block } if *chain == chain_id && *block == block_id)),
        "expected BlockReplaced event"
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let blk = chain.blocks.iter().find(|b| b.id == block_id).unwrap();
    assert_eq!(blk.id, block_id, "original id must be preserved");
    assert!(!blk.enabled, "enabled must be updated");
}

#[test]
fn overwrite_block_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::OverwriteBlock {
        chain: ChainId("chain_MISSING".to_string()),
        block: BlockId("blk_x".to_string()),
        replacement: make_core_block("blk_x", true),
    });

    assert!(result.is_err(), "expected Err for missing chain");
}

#[test]
fn overwrite_block_non_existent_block_returns_err() {
    let (project, chain_id) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::OverwriteBlock {
        chain: chain_id.clone(),
        block: BlockId("blk_MISSING".to_string()),
        replacement: make_core_block("blk_x", true),
    });

    assert!(result.is_err(), "expected Err for missing block");
}

// ── SaveChainIo tests ─────────────────────────────────────────────────────────

#[test]
fn save_chain_io_replaces_both_endpoints_and_emits_event() {
    let (project, chain_id) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_input = make_input_block("dev_in_new", 2);
    let new_output = make_output_block("dev_out_new", 7);

    let result = dispatcher.dispatch(Command::SaveChainIo {
        chain: chain_id.clone(),
        input_block: new_input,
        output_block: new_output,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainIoSaved { chain } if *chain == chain_id)),
        "expected ChainIoSaved, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let inp = chain.blocks.iter().find_map(|b| {
        if let AudioBlockKind::Input(ib) = &b.kind {
            Some(ib)
        } else {
            None
        }
    });
    let out = chain.blocks.iter().find_map(|b| {
        if let AudioBlockKind::Output(ob) = &b.kind {
            Some(ob)
        } else {
            None
        }
    });
    assert!(inp.is_some() && out.is_some());
    assert_eq!(inp.unwrap().entries[0].device_id.0, "dev_in_new");
    assert_eq!(out.unwrap().entries[0].device_id.0, "dev_out_new");
}

#[test]
fn save_chain_io_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainIo {
        chain: ChainId("chain_MISSING".to_string()),
        input_block: make_input_block("dev_in", 0),
        output_block: make_output_block("dev_out", 0),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

#[test]
fn save_chain_io_missing_input_block_returns_err() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("chain_no_input".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            blocks: vec![make_output_block("dev_b", 1)], // output only, no input
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainIo {
        chain: ChainId("chain_no_input".to_string()),
        input_block: make_input_block("dev_in", 0),
        output_block: make_output_block("dev_out", 0),
    });

    assert!(result.is_err(), "expected Err when chain has no InputBlock");
}

// ── SaveInsertBlock tests ─────────────────────────────────────────────────────

fn make_insert_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Insert(project::block::InsertBlock {
            model: "standard".to_string(),
            send: project::block::InsertEndpoint {
                device_id: DeviceId("send_dev".to_string()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
            return_: project::block::InsertEndpoint {
                device_id: DeviceId("return_dev".to_string()),
                mode: ChainInputMode::Mono,
                channels: vec![1],
            },
        }),
    }
}

fn make_project_with_insert() -> (Rc<RefCell<Project>>, ChainId, BlockId) {
    let insert = make_insert_block("insert_0");
    let block_id = insert.id.clone();
    let chain_id = ChainId("chain_insert".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            blocks: vec![insert],
        }],
        midi: None,
    }));
    (project, chain_id, block_id)
}

#[test]
fn save_insert_block_updates_endpoints_and_emits_event() {
    let (project, chain_id, block_id) = make_project_with_insert();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_send = project::block::InsertEndpoint {
        device_id: DeviceId("new_send_dev".to_string()),
        mode: ChainInputMode::Mono,
        channels: vec![2],
    };
    let new_return = project::block::InsertEndpoint {
        device_id: DeviceId("new_return_dev".to_string()),
        mode: ChainInputMode::Mono,
        channels: vec![3],
    };

    let result = dispatcher.dispatch(Command::SaveInsertBlock {
        chain: chain_id.clone(),
        block: block_id.clone(),
        send: new_send,
        return_: new_return,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::InsertBlockSaved { chain, block }
            if *chain == chain_id && *block == block_id
        )),
        "expected InsertBlockSaved event, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let block = chain.blocks.iter().find(|b| b.id == block_id).unwrap();
    if let AudioBlockKind::Insert(ib) = &block.kind {
        assert_eq!(ib.send.device_id.0, "new_send_dev");
        assert_eq!(ib.send.channels, vec![2]);
        assert_eq!(ib.return_.device_id.0, "new_return_dev");
        assert_eq!(ib.return_.channels, vec![3]);
    } else {
        panic!("expected InsertBlock kind");
    }
}

#[test]
fn save_insert_block_non_existent_block_returns_err() {
    let (project, chain_id, _) = make_project_with_insert();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveInsertBlock {
        chain: chain_id,
        block: BlockId("blk_MISSING".to_string()),
        send: project::block::InsertEndpoint {
            device_id: DeviceId("dev".to_string()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
        return_: project::block::InsertEndpoint {
            device_id: DeviceId("dev".to_string()),
            mode: ChainInputMode::Mono,
            channels: vec![1],
        },
    });

    assert!(result.is_err(), "expected Err for missing block, got Ok");
}

#[test]
fn save_insert_block_non_insert_kind_returns_err() {
    // Block exists but is a CoreBlock, not InsertBlock.
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveInsertBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        send: project::block::InsertEndpoint {
            device_id: DeviceId("dev".to_string()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
        return_: project::block::InsertEndpoint {
            device_id: DeviceId("dev".to_string()),
            mode: ChainInputMode::Mono,
            channels: vec![1],
        },
    });

    assert!(result.is_err(), "expected Err for non-Insert block kind");
}

// ── UpdateProjectName tests ───────────────────────────────────────────────────

fn make_named_project(name: &str) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: Some(name.to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }))
}

#[test]
fn update_project_name_writes_name_and_emits_event() {
    let project = make_named_project("old name");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::UpdateProjectName {
        name: "new name".to_string(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated event"
    );
    assert_eq!(
        project.borrow().name.as_deref(),
        Some("new name"),
        "project name must be updated"
    );
}

#[test]
fn update_project_name_sets_name_to_none_when_empty() {
    // An empty name should set project.name = None.
    let project = make_named_project("old name");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::UpdateProjectName {
        name: "".to_string(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    assert_eq!(
        project.borrow().name,
        None,
        "empty name must set project.name = None"
    );
}

#[test]
fn update_project_name_trims_whitespace() {
    let project = make_named_project("old");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::UpdateProjectName {
        name: "  trimmed  ".to_string(),
    });

    assert!(result.is_ok());
    assert_eq!(
        project.borrow().name.as_deref(),
        Some("trimmed"),
        "project name must be trimmed"
    );
}

// ── SaveAudioSettings tests ───────────────────────────────────────────────────

fn make_device_settings(device_id: &str) -> project::device::DeviceSettings {
    project::device::DeviceSettings {
        device_id: DeviceId(device_id.to_string()),
        sample_rate: 44100,
        buffer_size_frames: 256,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

#[test]
fn save_audio_settings_writes_device_settings_and_emits_event() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let settings = vec![make_device_settings("dev_a"), make_device_settings("dev_b")];
    let result = dispatcher.dispatch(Command::SaveAudioSettings {
        device_settings: settings.clone(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::AudioSettingsSaved)),
        "expected AudioSettingsSaved event, got {:?}",
        events
    );
    let proj = project.borrow();
    assert_eq!(proj.device_settings.len(), 2);
    assert_eq!(proj.device_settings[0].device_id.0, "dev_a");
    assert_eq!(proj.device_settings[1].device_id.0, "dev_b");
}

#[test]
fn save_audio_settings_replaces_previous_settings() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![make_device_settings("old_dev")],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveAudioSettings {
        device_settings: vec![make_device_settings("new_dev")],
    });

    assert!(result.is_ok());
    let proj = project.borrow();
    assert_eq!(proj.device_settings.len(), 1);
    assert_eq!(proj.device_settings[0].device_id.0, "new_dev");
}

#[test]
fn save_audio_settings_empty_clears_settings() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![make_device_settings("dev_a")],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveAudioSettings {
        device_settings: vec![],
    });

    assert!(result.is_ok());
    assert!(project.borrow().device_settings.is_empty());
}

// ── SaveProject tests ─────────────────────────────────────────────────────────

#[test]
fn save_project_emits_project_saved_event() {
    // SaveProject in the dispatcher is a no-op metadata signal — actual file I/O
    // happens in the adapter. The dispatcher just emits ProjectSaved.
    let project = Rc::new(RefCell::new(Project {
        name: Some("my project".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveProject);

    assert!(result.is_ok(), "SaveProject must return Ok: {:?}", result);
    assert!(
        result
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::ProjectSaved)),
        "expected ProjectSaved event"
    );
}

#[test]
fn save_project_does_not_mutate_project() {
    let project = Rc::new(RefCell::new(Project {
        name: Some("stable".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let _ = dispatcher.dispatch(Command::SaveProject);

    assert_eq!(
        project.borrow().name.as_deref(),
        Some("stable"),
        "SaveProject must not mutate the project"
    );
}

#[test]
fn save_project_is_ok_with_empty_project() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let result = dispatcher.dispatch(Command::SaveProject);
    assert!(result.is_ok());
}

// ── LoadProject tests ─────────────────────────────────────────────────────────

#[test]
fn load_project_replaces_project_and_emits_event() {
    let project = Rc::new(RefCell::new(Project {
        name: Some("old".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_proj = Project {
        name: Some("loaded".to_string()),
        device_settings: vec![],
        chains: vec![make_empty_chain("chain_loaded", false)],
        midi: None,
    };

    let result = dispatcher.dispatch(Command::LoadProject {
        project: new_proj,
        path: std::path::PathBuf::from("/some/path.yaml"),
    });

    assert!(result.is_ok(), "LoadProject returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectLoaded)),
        "expected ProjectLoaded event, got {:?}",
        events
    );
    let proj = project.borrow();
    assert_eq!(proj.name.as_deref(), Some("loaded"));
    assert_eq!(proj.chains.len(), 1);
    assert_eq!(proj.chains[0].id.0, "chain_loaded");
}

#[test]
fn load_project_replaces_all_state() {
    // Project starts with chains; load should replace them entirely.
    let project = Rc::new(RefCell::new(Project {
        name: Some("old".to_string()),
        device_settings: vec![make_device_settings("old_dev")],
        chains: vec![make_empty_chain("old_chain", false)],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_proj = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };

    let _ = dispatcher.dispatch(Command::LoadProject {
        project: new_proj,
        path: std::path::PathBuf::from("/p.yaml"),
    });

    let proj = project.borrow();
    assert!(proj.name.is_none());
    assert!(proj.device_settings.is_empty());
    assert!(proj.chains.is_empty());
}

#[test]
fn load_project_emits_project_mutated() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::LoadProject {
        project: Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        path: std::path::PathBuf::from("/p.yaml"),
    });

    assert!(result.is_ok());
    assert!(
        result
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated"
    );
}

// ── CreateProject tests ───────────────────────────────────────────────────────

#[test]
fn create_project_replaces_project_and_emits_event() {
    let project = Rc::new(RefCell::new(Project {
        name: Some("old".to_string()),
        device_settings: vec![],
        chains: vec![make_empty_chain("old_chain", false)],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_proj = Project {
        name: Some("brand new".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };

    let result = dispatcher.dispatch(Command::CreateProject { project: new_proj });

    assert!(result.is_ok(), "CreateProject returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectCreated)),
        "expected ProjectCreated event, got {:?}",
        events
    );
    let proj = project.borrow();
    assert_eq!(proj.name.as_deref(), Some("brand new"));
    assert!(proj.chains.is_empty());
}

#[test]
fn create_project_emits_project_mutated() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::CreateProject {
        project: Project {
            name: Some("new".to_string()),
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
    });

    assert!(result.is_ok());
    assert!(
        result
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated"
    );
}

#[test]
fn create_project_replaces_all_prior_state() {
    let project = Rc::new(RefCell::new(Project {
        name: Some("old".to_string()),
        device_settings: vec![make_device_settings("dev_old")],
        chains: vec![make_empty_chain("c", false)],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let _ = dispatcher.dispatch(Command::CreateProject {
        project: Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
    });

    let proj = project.borrow();
    assert!(proj.name.is_none());
    assert!(proj.device_settings.is_empty());
    assert!(proj.chains.is_empty());
}

// ── LoadChainPreset tests ─────────────────────────────────────────────────────

#[test]
fn load_chain_preset_replaces_blocks_and_emits_event() {
    let existing_block = make_core_block("blk_old", true);
    let project = make_project("chain_preset", existing_block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_block_a = make_core_block("blk_new_a", true);
    let new_block_b = make_core_block("blk_new_b", false);
    let preset_blocks = vec![new_block_a, new_block_b];

    let result = dispatcher.dispatch(Command::LoadChainPreset {
        chain: ChainId("chain_preset".to_string()),
        preset_blocks: preset_blocks.clone(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainPresetLoaded { chain } if chain.0 == "chain_preset")),
        "expected ChainPresetLoaded event, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj
        .chains
        .iter()
        .find(|c| c.id.0 == "chain_preset")
        .unwrap();
    assert_eq!(chain.blocks.len(), 2, "chain should have 2 preset blocks");
    assert_eq!(chain.blocks[0].id.0, "blk_new_a");
    assert_eq!(chain.blocks[1].id.0, "blk_new_b");
}

#[test]
fn load_chain_preset_non_existent_chain_returns_err() {
    let block = make_core_block("blk_0", true);
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::LoadChainPreset {
        chain: ChainId("chain_MISSING".to_string()),
        preset_blocks: vec![make_core_block("new_blk", true)],
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    // Original chain must be unchanged
    let proj = project.borrow();
    assert_eq!(proj.chains[0].blocks.len(), 1, "chain must not be mutated");
    assert_eq!(proj.chains[0].blocks[0].id.0, "blk_0");
}

#[test]
fn load_chain_preset_empty_blocks_succeeds() {
    let existing_block = make_core_block("blk_old", true);
    let project = make_project("chain_preset", existing_block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // A zero-block preset is valid — the chain becomes empty.
    let result = dispatcher.dispatch(Command::LoadChainPreset {
        chain: ChainId("chain_preset".to_string()),
        preset_blocks: vec![],
    });

    assert!(
        result.is_ok(),
        "empty preset should succeed, got {:?}",
        result
    );
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainPresetLoaded { chain } if chain.0 == "chain_preset")),
        "expected ChainPresetLoaded even for empty preset, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj
        .chains
        .iter()
        .find(|c| c.id.0 == "chain_preset")
        .unwrap();
    assert!(
        chain.blocks.is_empty(),
        "chain should be empty after empty preset load"
    );
}

// ── Invariant: dispatch must not be called while caller holds a Project borrow ────

#[test]
fn dispatch_panics_if_caller_holds_external_immutable_borrow_of_project() {
    // Reproduces the real bug we hit in adapter-gui (recent_projects_wiring /
    // project_file_dialog_wiring): building the Command's `project` field via
    // `project_rc.borrow().clone()` keeps the immutable borrow alive until the
    // dispatch call returns. Inside dispatch, `self.project.borrow_mut()`
    // then panics with "RefCell already borrowed".
    //
    // This test pins the invariant: callers MUST drop every project borrow
    // BEFORE invoking dispatch. The fix on the adapter side is to clone into
    // a local variable first.
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    let project_rc = Rc::new(RefCell::new(project));
    let dispatcher = LocalDispatcher::new(project_rc.clone());

    let _alive = project_rc.borrow(); // simulates an adapter holding `&Project`

    let new_project = Project {
        name: Some("loaded".into()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatcher.dispatch(Command::LoadProject {
            project: new_project,
            path: std::path::PathBuf::from("/tmp/x"),
        })
    }));

    assert!(
        result.is_err(),
        "dispatch is expected to panic when caller still holds a project borrow"
    );
}

#[test]
fn dispatch_succeeds_when_caller_drops_borrow_before_calling() {
    // Positive companion: after the borrow is dropped, dispatch goes through.
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    let project_rc = Rc::new(RefCell::new(project));
    let dispatcher = LocalDispatcher::new(project_rc.clone());

    let snapshot = project_rc.borrow().clone(); // takes the borrow temporarily
    drop(snapshot); // (actually, .clone() already returns Project by value;
                    // the temporary borrow is gone after this line.)

    let new_project = Project {
        name: Some("loaded".into()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    let result = dispatcher.dispatch(Command::LoadProject {
        project: new_project,
        path: std::path::PathBuf::from("/tmp/x"),
    });

    assert!(
        result.is_ok(),
        "dispatch without live borrow should succeed"
    );
    assert_eq!(
        project_rc.borrow().name.as_deref(),
        Some("loaded"),
        "loaded project replaced shared state"
    );
}

// ── SetChainVolume (issue #440, port to #295 command bus) ────────────────────

fn make_project_with_volume(chain_id: &str, volume: f32) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(chain_id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume,
            blocks: vec![],
        }],
        midi: None,
    }))
}

#[test]
fn set_chain_volume_updates_value_and_emits_event() {
    let project = make_project_with_volume("chain_0", 100.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetChainVolume {
        chain: ChainId("chain_0".to_string()),
        value: 150.0,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    // Must emit ChainVolumeChanged + ProjectMutated
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainVolumeChanged {
                chain,
                value,
            } if chain.0 == "chain_0" && (*value - 150.0).abs() < f32::EPSILON
        )),
        "expected ChainVolumeChanged event; got: {:?}",
        events
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated event; got: {:?}",
        events
    );
    assert!(
        (project.borrow().chains[0].volume - 150.0).abs() < f32::EPSILON,
        "chain.volume should be 150.0 after dispatch, got {}",
        project.borrow().chains[0].volume
    );
}

#[test]
fn set_chain_volume_non_existent_chain_returns_err() {
    let project = make_project_with_volume("chain_0", 100.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetChainVolume {
        chain: ChainId("chain_MISSING".to_string()),
        value: 150.0,
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert!(
        (project.borrow().chains[0].volume - 100.0).abs() < f32::EPSILON,
        "volume must not be mutated when chain not found"
    );
}

#[test]
fn set_chain_volume_passes_extreme_values_verbatim() {
    // Policy: no clamp. Values 0.0 and 250.0 stored as-is.
    let project = make_project_with_volume("chain_0", 100.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::SetChainVolume {
            chain: ChainId("chain_0".to_string()),
            value: 0.0,
        })
        .expect("dispatch 0.0 should succeed");
    assert!(
        project.borrow().chains[0].volume.abs() < f32::EPSILON,
        "volume=0.0 should be stored verbatim"
    );

    dispatcher
        .dispatch(Command::SetChainVolume {
            chain: ChainId("chain_0".to_string()),
            value: 250.0,
        })
        .expect("dispatch 250.0 should succeed");
    assert!(
        (project.borrow().chains[0].volume - 250.0).abs() < f32::EPSILON,
        "volume=250.0 should be stored verbatim"
    );
}

// ── #513 / #493: MIDI device + mapping + learn commands ──────────────────────

fn empty_project_rc() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }))
}

#[test]
fn save_midi_devices_emits_event_without_mutating_project() {
    let project = empty_project_rc();
    let before = project.borrow().clone();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::SaveMidiDevices { devices: vec![] })
        .unwrap();

    assert_eq!(events, vec![Event::MidiDevicesSaved]);
    assert_eq!(
        project.borrow().chains.len(),
        before.chains.len(),
        "system command must not touch project chains"
    );
    assert_eq!(
        project.borrow().device_settings.len(),
        before.device_settings.len(),
        "system command must not touch project device_settings"
    );
    assert_eq!(
        project.borrow().name,
        before.name,
        "system command must not touch project name"
    );
}

#[test]
fn save_midi_mapping_writes_bindings_into_project_midi() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let bindings = vec![project::midi::Binding {
        source: project::midi::Source::ProgramChange { program: 7 },
        command: "SaveProject".to_string(),
        args: serde_json::Value::Null,
        scale: None,
    }];

    let events = dispatcher
        .dispatch(Command::SaveMidiMapping {
            bindings: bindings.clone(),
        })
        .unwrap();

    assert_eq!(events, vec![Event::MidiMappingSaved, Event::ProjectMutated]);
    let stored = project
        .borrow()
        .midi
        .clone()
        .unwrap_or_default()
        .bindings
        .clone();
    assert_eq!(stored, bindings);
}

#[test]
fn start_and_stop_midi_learn_emit_events() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    assert_eq!(
        dispatcher.dispatch(Command::StartMidiLearn).unwrap(),
        vec![Event::MidiLearnStarted]
    );
    assert_eq!(
        dispatcher.dispatch(Command::StopMidiLearn).unwrap(),
        vec![Event::MidiLearnStopped]
    );
}

#[test]
fn publish_midi_event_passthrough_emits_midi_event_received() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let source = project::midi::Source::Cc {
        channel: 1,
        controller: 7,
    };
    let events = dispatcher
        .dispatch(Command::PublishMidiEvent {
            source: source.clone(),
        })
        .unwrap();
    assert_eq!(events, vec![Event::MidiEventReceived { source }]);
}

// ── #513: System / Paths (presets + plugins) ─────────────────────────────────
//
// RED-FIRST tests: SetPresetsPath and SetPluginsPath are user-visible Settings
// commands that update the system-level AssetPaths snapshot. They emit
// PathsSaved so the adapter can persist config.yaml.

#[test]
fn set_presets_path_emits_paths_saved() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let path = std::path::PathBuf::from("/tmp/openrig-test-presets");

    let events = dispatcher
        .dispatch(Command::SetPresetsPath {
            path: Some(path.clone()),
        })
        .unwrap();

    assert_eq!(events, vec![Event::PathsSaved]);
    // System command must not touch the project itself.
    assert!(project.borrow().chains.is_empty());
    assert!(project.borrow().midi.is_none());
}

#[test]
fn set_plugins_path_emits_paths_saved() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let path = std::path::PathBuf::from("/tmp/openrig-test-plugins");

    let events = dispatcher
        .dispatch(Command::SetPluginsPath {
            path: Some(path.clone()),
        })
        .unwrap();

    assert_eq!(events, vec![Event::PathsSaved]);
    assert!(project.borrow().chains.is_empty());
    assert!(project.borrow().midi.is_none());
}

#[test]
fn set_presets_path_none_resets_to_default_and_still_emits() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::SetPresetsPath { path: None })
        .unwrap();
    assert_eq!(events, vec![Event::PathsSaved]);
}

#[test]
fn set_plugins_path_none_resets_to_default_and_still_emits() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::SetPluginsPath { path: None })
        .unwrap();
    assert_eq!(events, vec![Event::PathsSaved]);
}
