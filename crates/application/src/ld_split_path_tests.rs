//! #328 — block commands address blocks inside a split's paths.
//!
//! Every test drives the dispatcher the way a transport does. Fixtures live in
//! `split_tests_fixtures.rs`.

use project::block::split_param_keys::mix_pan;
use project::block::split_params::{MIX_MASTER_SUM, SPLIT_MODE};
use project::block::SplitEnd;
use serde_json::json;

use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

#[test]
fn set_block_parameter_number_reaches_a_block_inside_path_b() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::SetBlockParameterNumber {
            chain: ChainId(CHAIN.into()),
            block: BlockId("b_0".into()),
            path: "gain".into(),
            value: 0.25,
        }))
        .expect("a block inside path B is addressable by id");

    let split = split_of(&project.borrow());
    let AudioBlockKind::Core(core) = &split.paths[1][0].kind else {
        panic!("b_0 is a core block");
    };
    assert_eq!(core.params.get_f32("gain"), Some(0.25));
}

#[test]
fn toggle_block_enabled_reaches_a_block_inside_path_a() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
            chain: ChainId(CHAIN.into()),
            block: BlockId("a_0".into()),
        }))
        .expect("a block inside path A is addressable by id");

    assert!(
        !split_of(&project.borrow()).paths[0][0].enabled,
        "a_0 was switched off"
    );
}

#[test]
fn set_block_parameter_number_writes_a_split_mixer_knob() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::SetBlockParameterNumber {
            chain: ChainId(CHAIN.into()),
            block: BlockId("split_0".into()),
            path: mix_pan(0),
            value: -50.0,
        }))
        .expect("the split's knobs are ordinary block parameters");

    assert_eq!(
        split_of(&project.borrow()).params.get_f32(&mix_pan(0)),
        Some(-50.0)
    );
}

#[test]
fn set_block_parameter_number_refuses_a_split_knob_outside_its_range() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::SetBlockParameterNumber {
        chain: ChainId(CHAIN.into()),
        block: BlockId("split_0".into()),
        path: mix_pan(0),
        value: 100.0,
    }));

    assert!(result.is_err(), "pan runs -50..50, 100 must be refused");
    assert_eq!(
        split_of(&project.borrow()).params.get_f32(&mix_pan(0)),
        Some(0.0),
        "a refused value leaves the knob untouched"
    );
}

#[test]
fn select_block_parameter_option_switches_the_split_mode() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::SelectBlockParameterOption {
            chain: ChainId(CHAIN.into()),
            block: BlockId("split_0".into()),
            path: SPLIT_MODE.into(),
            value: "dual_mono".into(),
            index: 1,
        }))
        .expect("split_mode is an option parameter of the split");

    assert_eq!(
        split_of(&project.borrow()).params.get_string(SPLIT_MODE),
        Some("dual_mono")
    );
}

#[test]
fn set_block_parameter_bool_turns_on_the_master_sum() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::SetBlockParameterBool {
            chain: ChainId(CHAIN.into()),
            block: BlockId("split_0".into()),
            path: MIX_MASTER_SUM.into(),
            value: true,
        }))
        .expect("mix_master_sum is a bool parameter of the split");

    assert_eq!(
        split_of(&project.borrow()).params.get_bool(MIX_MASTER_SUM),
        Some(true)
    );
}

#[test]
fn remove_block_takes_a_block_out_of_path_a() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::Block(BlockCommand::RemoveBlock {
            chain: ChainId(CHAIN.into()),
            block: BlockId("a_0".into()),
        }))
        .expect("a block inside path A can be removed");

    assert!(
        split_of(&project.borrow()).paths[0].is_empty(),
        "path A is empty"
    );
    assert_eq!(
        ids(&project.borrow().chains[0].blocks),
        vec!["pre", "split_0", "post"]
    );
    assert_eq!(
        events,
        vec![Event::BlockRemoved {
            chain: ChainId(CHAIN.into()),
            block: BlockId("a_0".into()),
        }]
    );
}

#[test]
fn remove_block_refuses_the_split_and_leaves_the_chain_untouched() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatcher
        .dispatch(Command::Block(BlockCommand::RemoveBlock {
            chain: ChainId(CHAIN.into()),
            block: BlockId("split_0".into()),
        }))
        .expect_err("RemoveBlock must not drop both paths");

    assert!(err.to_string().contains("RemoveSplit"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn overwrite_block_refuses_to_put_an_input_port_inside_a_path() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();
    let port = AudioBlock {
        id: BlockId("ignored".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };

    let err = dispatcher
        .dispatch(Command::Block(BlockCommand::OverwriteBlock {
            chain: ChainId(CHAIN.into()),
            block: BlockId("b_0".into()),
            replacement: port,
        }))
        .expect_err("an input port cannot sit inside a path");

    // One set of rule texts (README orchestrator decision 6): Part 1's
    // `SplitBlock::validate_structure` names the offending path block.
    assert!(
        err.to_string()
            .contains("'b_0' is a input block; a path holds no port or insert"),
        "{err}"
    );
    assert_eq!(project.borrow().chains[0].blocks, before);
}

/// Characterization pin (green since Task 1): an ordinary overwrite inside a
/// path keeps working once OverwriteBlock goes through the rule-checked draft.
#[test]
fn overwrite_block_replaces_a_block_inside_path_b() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::OverwriteBlock {
            chain: ChainId(CHAIN.into()),
            block: BlockId("b_0".into()),
            replacement: make_core_block("ignored", false),
        }))
        .expect("a core block may replace a core block inside a path");

    let split = split_of(&project.borrow());
    assert_eq!(split.paths[1][0].id.0, "b_0", "the original id is kept");
    assert!(!split.paths[1][0].enabled, "the replacement's state landed");
}

fn added_id(events: &[Event]) -> BlockId {
    events
        .iter()
        .find_map(|e| match e {
            Event::BlockAdded { block, .. } => Some(block.clone()),
            _ => None,
        })
        .expect("the command answers BlockAdded")
}

#[test]
fn add_block_with_a_path_lands_inside_that_path() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({
            "chain": CHAIN, "kind": "gain", "model_id": "fuzz_ge", "position": 0,
            "path": { "split": "split_0", "path": 0 }
        }),
    )
    .expect("AddBlock into path A");

    let split = split_of(&project.borrow());
    assert_eq!(split.paths[0].len(), 2, "path A = [new, a_0]");
    assert_eq!(split.paths[0][1].id.0, "a_0");
    assert_eq!(added_id(&events), split.paths[0][0].id);
    assert_eq!(
        ids(&project.borrow().chains[0].blocks),
        vec!["pre", "split_0", "post"],
        "the top level is untouched"
    );
}

/// Characterization pin (green before and after): a path-less AddBlock is a
/// top-level add, exactly as every pre-#328 payload expects.
#[test]
fn add_block_without_a_path_still_lands_at_the_top_level() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({ "chain": CHAIN, "kind": "gain", "model_id": "fuzz_ge", "position": 0 }),
    )
    .expect("a top-level AddBlock");

    let blocks = project.borrow().chains[0].blocks.clone();
    assert_eq!(blocks.len(), 4);
    assert_eq!(ids(&blocks[1..]), vec!["pre", "split_0", "post"]);
    assert_eq!(
        split_of(&project.borrow()).paths[0].len(),
        1,
        "path A is untouched"
    );
}

#[test]
fn add_block_refuses_an_input_port_inside_a_path() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({
            "chain": CHAIN, "kind": "input", "model_id": "standard", "position": 0,
            "path": { "split": "split_0", "path": 1 }
        }),
    )
    .expect_err("an input port cannot sit inside a path");

    assert!(
        err.to_string().contains("a path holds no port or insert"),
        "{err}"
    );
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn add_block_refuses_a_processing_block_after_a_y_split() {
    let project = project_with(y_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({ "chain": CHAIN, "kind": "gain", "model_id": "fuzz_ge", "position": 2 }),
    )
    .expect_err("nothing may follow a Y split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn add_block_never_reuses_an_id_after_a_removal() {
    let project = project_with(vec![make_core_block("blk_0", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let add = || {
        dispatch_json(
            &dispatcher,
            "AddBlock",
            json!({ "chain": CHAIN, "kind": "gain", "model_id": "fuzz_ge", "position": 99 }),
        )
        .expect("AddBlock")
    };

    let first = added_id(&add());
    dispatcher
        .dispatch(Command::Block(BlockCommand::RemoveBlock {
            chain: ChainId(CHAIN.into()),
            block: BlockId("blk_0".into()),
        }))
        .expect("remove blk_0");
    let second = added_id(&add());

    assert_ne!(
        first, second,
        "a new block must never reuse an id in the chain"
    );
    assert!(second.0.starts_with("chain_0:block:"), "{}", second.0);
}

#[test]
fn insert_prebuilt_block_with_a_path_lands_inside_path_b() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let block = serde_json::to_value(make_core_block("pre_b", true)).expect("block serializes");

    dispatch_json(
        &dispatcher,
        "InsertPrebuiltBlock",
        json!({
            "chain": CHAIN, "block": block, "position": 5,
            "path": { "split": "split_0", "path": 1 }
        }),
    )
    .expect("InsertPrebuiltBlock into path B");

    assert_eq!(
        ids(&split_of(&project.borrow()).paths[1]),
        vec!["b_0", "pre_b"]
    );
    assert_eq!(project.borrow().chains[0].blocks.len(), 3);
}

/// Spec §10.1: no split count — a second Mix goes in like any block.
#[test]
fn insert_prebuilt_block_accepts_a_second_split() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let second = serde_json::to_value(split("split_1", SplitEnd::Mix, vec![], vec![]))
        .expect("split serializes");

    dispatch_json(
        &dispatcher,
        "InsertPrebuiltBlock",
        json!({ "chain": CHAIN, "block": second, "position": 0 }),
    )
    .expect("a chain holds any number of splits");

    assert_eq!(project.borrow().chains[0].blocks[0].id.0, "split_1");
    assert_eq!(project.borrow().chains[0].blocks.len(), 4);
}

#[test]
fn add_block_into_a_path_reaches_the_rig_preset_on_capture() {
    let (rig, _project, dispatcher) = rig_session_from(rig_with_presets(vec![(
        "p1",
        vec![
            make_core_block("A", true),
            split("S", SplitEnd::Mix, vec![], vec![]),
        ],
    )]));

    dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({
            "chain": RIG_CHAIN, "kind": "gain", "model_id": "fuzz_ge", "position": 0,
            "path": { "split": "S", "path": 0 }
        }),
    )
    .expect("AddBlock into path A of the rig chain");
    dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture");

    assert_eq!(
        preset_split(&rig.borrow(), "p1").paths[0].len(),
        1,
        "the block added into path A must reach the project file"
    );
}

#[test]
fn move_block_drops_a_top_level_block_into_path_b() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "MoveBlock",
        json!({
            "chain": CHAIN, "block": "pre", "new_position": 0,
            "path": { "split": "split_0", "path": 1 }
        }),
    )
    .expect("move pre into path B");

    assert_eq!(
        ids(&project.borrow().chains[0].blocks),
        vec!["split_0", "post"]
    );
    assert_eq!(
        ids(&split_of(&project.borrow()).paths[1]),
        vec!["pre", "b_0"]
    );
}

#[test]
fn move_block_drags_a_block_from_path_a_to_path_b() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatch_json(
        &dispatcher,
        "MoveBlock",
        json!({
            "chain": CHAIN, "block": "a_0", "new_position": 1,
            "path": { "split": "split_0", "path": 1 }
        }),
    )
    .expect("drag a_0 across the split");

    let split = split_of(&project.borrow());
    assert!(split.paths[0].is_empty(), "path A is empty");
    assert_eq!(ids(&split.paths[1]), vec!["b_0", "a_0"]);
    assert_eq!(
        events,
        vec![Event::ChainReloaded {
            chain: ChainId(CHAIN.into())
        }]
    );
}

#[test]
fn move_block_without_a_path_lifts_a_path_block_to_the_top_level() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "MoveBlock",
        json!({ "chain": CHAIN, "block": "b_0", "new_position": 0 }),
    )
    .expect("lift b_0 out of path B");

    assert_eq!(
        ids(&project.borrow().chains[0].blocks),
        vec!["b_0", "pre", "split_0", "post"]
    );
    assert!(
        split_of(&project.borrow()).paths[1].is_empty(),
        "path B is empty"
    );
}

#[test]
fn move_block_refuses_to_put_the_split_inside_its_own_path() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "MoveBlock",
        json!({
            "chain": CHAIN, "block": "split_0", "new_position": 0,
            "path": { "split": "split_0", "path": 0 }
        }),
    )
    .expect_err("a split cannot go inside its own path");

    assert!(err.to_string().contains("split not found"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn move_block_refuses_a_processing_block_after_a_y_split() {
    let project = project_with(y_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "MoveBlock",
        json!({ "chain": CHAIN, "block": "pre", "new_position": 9 }),
    )
    .expect_err("nothing may follow a Y split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn add_block_into_the_y_path_lands_in_the_y_behind_a_mix() {
    let project = project_with(mix_then_y_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({
            "chain": CHAIN, "kind": "gain", "model_id": "fuzz_ge", "position": 1,
            "path": { "split": "y", "path": 0 }
        }),
    )
    .expect("AddBlock into path A of the Y");

    let y = split_by_id(&project.borrow(), "y");
    assert_eq!(y.paths[0].len(), 2, "path A of the Y = [ya_0, new]");
    assert_eq!(added_id(&events), y.paths[0][1].id);
    assert_eq!(
        ids(&split_by_id(&project.borrow(), "mix").paths[0]),
        vec!["a_0"],
        "the Mix's paths are untouched"
    );
}

#[test]
fn add_block_refuses_an_input_port_in_the_y_path_behind_a_mix() {
    let project = project_with(mix_then_y_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({
            "chain": CHAIN, "kind": "input", "model_id": "standard", "position": 0,
            "path": { "split": "y", "path": 1 }
        }),
    )
    .expect_err("every split's paths are checked, not only the first split's");

    assert!(
        err.to_string().contains("a path holds no port or insert"),
        "{err}"
    );
    assert_eq!(project.borrow().chains[0].blocks, before);
}
