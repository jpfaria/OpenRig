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
            blocks: vec![block],
        }],
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
            blocks: vec![
                make_core_block("blk_0", true),
                make_core_block("blk_1", true),
            ],
        }],
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
            blocks: vec![
                make_core_block("blk_0", true),
                make_core_block("blk_1", true),
                make_core_block("blk_2", true),
            ],
        }],
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

// ── Chain-level test helpers ──────────────────────────────────────────────────

/// Build a chain with an InputBlock on device `dev_id`, channel `ch`.
fn make_chain_with_input(chain_id: &str, dev_id: &str, ch: usize, enabled: bool) -> Chain {
    Chain {
        id: ChainId(chain_id.to_string()),
        description: Some(chain_id.to_string()),
        instrument: "electric_guitar".to_string(),
        enabled,
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
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

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
