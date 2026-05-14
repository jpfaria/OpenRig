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

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
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
    assert_eq!(proj.chains[0].blocks.len(), 1, "chain must have 1 block after remove");
    assert_eq!(proj.chains[0].blocks[0].id.0, "blk_1", "remaining block must be blk_1");
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
    let ids: Vec<&str> = proj.chains[0].blocks.iter().map(|b| b.id.0.as_str()).collect();
    assert_eq!(ids, vec!["blk_2", "blk_0", "blk_1"], "blocks must be reordered");
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
    let ids: Vec<&str> = proj.chains[0].blocks.iter().map(|b| b.id.0.as_str()).collect();
    assert_eq!(ids, vec!["blk_1", "blk_2", "blk_0"], "block must be clamped to end");
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
    let ids: Vec<&str> = proj.chains[0].blocks.iter().map(|b| b.id.0.as_str()).collect();
    assert_eq!(ids, vec!["blk_0", "blk_1", "blk_2"], "order must not change when block not found");
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
    assert_eq!(proj.chains[0].blocks.len(), 2, "chain must have 2 blocks after add");
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
    assert_eq!(proj.chains[0].blocks.len(), 2, "chain must have 2 blocks after add");
    // The original block is still at position 0; new block is at the end
    assert_eq!(proj.chains[0].blocks[0].id.0, "blk_0", "original block stays first");
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
    assert_eq!(block.id.0, "blk_0", "block id must be preserved after model swap");
    assert!(!block.enabled, "block enabled state must be preserved after model swap");
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
