//! #328 — block commands address blocks inside a split's paths.
//!
//! Every test drives the dispatcher the way a transport does. Fixtures live in
//! `split_tests_fixtures.rs`.

use project::block::split_params::{MIX_MASTER_SUM, MIX_PAN_A, SPLIT_MODE};

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
    let AudioBlockKind::Core(core) = &split.b[0].kind else {
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
        !split_of(&project.borrow()).a[0].enabled,
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
            path: MIX_PAN_A.into(),
            value: -50.0,
        }))
        .expect("the split's knobs are ordinary block parameters");

    assert_eq!(
        split_of(&project.borrow()).params.get_f32(MIX_PAN_A),
        Some(-50.0)
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
