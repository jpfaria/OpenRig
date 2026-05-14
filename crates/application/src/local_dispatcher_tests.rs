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
        input_block: new_input,
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
    let input_block = chain
        .blocks
        .iter()
        .find(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
    assert!(input_block.is_some(), "chain must have an input block");
    if let Some(AudioBlockKind::Input(ib)) = input_block.map(|b| &b.kind) {
        assert_eq!(ib.entries[0].device_id.0, "dev_new");
        assert_eq!(ib.entries[0].channels, vec![3]);
    }
}

#[test]
fn save_chain_input_endpoints_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: ChainId("chain_MISSING".to_string()),
        input_block: make_input_block("dev_new", 0),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

#[test]
fn save_chain_input_endpoints_non_input_block_returns_err() {
    // Chain has no InputBlock at all — dispatcher must return Err.
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("chain_no_input".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            blocks: vec![make_core_block("blk_0", true)],
        }],
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: ChainId("chain_no_input".to_string()),
        input_block: make_input_block("dev_new", 0),
    });

    assert!(
        result.is_err(),
        "expected Err when chain has no InputBlock, got Ok"
    );
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
            blocks: vec![make_input_block("dev_a", 0), make_output_block("dev_b", 1)],
        }],
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
        output_block: new_output,
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
    let out_block = chain
        .blocks
        .iter()
        .find(|b| matches!(&b.kind, AudioBlockKind::Output(_)));
    assert!(out_block.is_some(), "chain must have an output block");
    if let Some(AudioBlockKind::Output(ob)) = out_block.map(|b| &b.kind) {
        assert_eq!(ob.entries[0].device_id.0, "dev_new_out");
        assert_eq!(ob.entries[0].channels, vec![5]);
    }
}

#[test]
fn save_chain_output_endpoints_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: ChainId("chain_MISSING".to_string()),
        output_block: make_output_block("dev_new_out", 0),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

#[test]
fn save_chain_output_endpoints_no_output_block_returns_err() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("chain_no_output".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            blocks: vec![make_core_block("blk_0", true)],
        }],
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: ChainId("chain_no_output".to_string()),
        output_block: make_output_block("dev_new_out", 0),
    });

    assert!(
        result.is_err(),
        "expected Err when chain has no OutputBlock"
    );
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
            blocks: vec![make_output_block("dev_b", 1)], // output only, no input
        }],
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
            blocks: vec![insert],
        }],
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

// ── SaveBlockEditorDraft (no-op) tests ────────────────────────────────────────

#[test]
fn save_block_editor_draft_is_noop_ok() {
    // SaveBlockEditorDraft is now a no-op. Should return Ok with no events.
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveBlockEditorDraft {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
    });

    assert!(result.is_ok(), "SaveBlockEditorDraft must return Ok");
    assert!(
        result.unwrap().is_empty(),
        "SaveBlockEditorDraft must produce no events"
    );
}

#[test]
fn save_block_editor_draft_missing_chain_still_noop() {
    // Even with a non-existent chain, SaveBlockEditorDraft is a no-op.
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveBlockEditorDraft {
        chain: ChainId("chain_MISSING".to_string()),
        block: BlockId("blk_0".to_string()),
    });

    assert!(
        result.is_ok(),
        "SaveBlockEditorDraft must be Ok even with missing chain"
    );
}

#[test]
fn save_block_editor_draft_produces_no_project_mutation() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let before = project.borrow().chains[0].blocks[0].enabled;
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let _ = dispatcher.dispatch(Command::SaveBlockEditorDraft {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
    });

    assert_eq!(
        project.borrow().chains[0].blocks[0].enabled,
        before,
        "SaveBlockEditorDraft must not mutate the project"
    );
}

// ── UpdateProjectName tests ───────────────────────────────────────────────────

fn make_named_project(name: &str) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: Some(name.to_string()),
        device_settings: vec![],
        chains: vec![],
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
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_proj = Project {
        name: Some("loaded".to_string()),
        device_settings: vec![],
        chains: vec![make_empty_chain("chain_loaded", false)],
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
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_proj = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
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
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::LoadProject {
        project: Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
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
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_proj = Project {
        name: Some("brand new".to_string()),
        device_settings: vec![],
        chains: vec![],
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
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::CreateProject {
        project: Project {
            name: Some("new".to_string()),
            device_settings: vec![],
            chains: vec![],
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
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let _ = dispatcher.dispatch(Command::CreateProject {
        project: Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
        },
    });

    let proj = project.borrow();
    assert!(proj.name.is_none());
    assert!(proj.device_settings.is_empty());
    assert!(proj.chains.is_empty());
}
