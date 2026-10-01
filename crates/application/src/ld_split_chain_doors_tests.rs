//! #328 — the commands that take a whole block list (a chain, a preset) are
//! doors too: a list that breaks a split rule is refused before anything —
//! project or rig — takes it.

use project::block::{SelectBlock, SplitEnd};

use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

fn chain_with(project: &Rc<RefCell<Project>>, blocks: Vec<AudioBlock>) -> Chain {
    let mut chain = project.borrow().chains[0].clone();
    chain.blocks = blocks;
    chain
}

/// Spec §10.1: no split count, but a Y still ends its own list.
#[test]
fn add_chain_refuses_a_chain_with_a_split_after_a_y() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let mut chain = chain_with(
        &project,
        vec![
            split("s1", SplitEnd::Y, vec![], vec![]),
            split("s2", SplitEnd::Mix, vec![], vec![]),
        ],
    );
    chain.id = ChainId("chain_new".into());
    chain.enabled = false;

    let err = dispatcher
        .dispatch(Command::Chain(ChainCommand::AddChain { chain }))
        .expect_err("a split after the Y");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(project.borrow().chains.len(), 1, "nothing was added");
}

#[test]
fn configure_chain_refuses_a_select_inside_a_path() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let select = AudioBlock {
        id: BlockId("sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("opt".into()),
            options: vec![make_core_block("opt", true)],
        }),
    };
    let chain = chain_with(
        &project,
        vec![split("s1", SplitEnd::Mix, vec![select], vec![])],
    );

    let err = dispatcher
        .dispatch(Command::Chain(ChainCommand::ConfigureChain { chain }))
        .expect_err("a select cannot sit inside a path");

    assert!(
        err.to_string()
            .contains("'sel' is a select block; a path holds processing blocks only"),
        "{err}"
    );
    assert_eq!(ids(&project.borrow().chains[0].blocks), vec!["amp"]);
}

#[test]
fn save_chain_refuses_a_block_after_a_y_split() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let chain = chain_with(
        &project,
        vec![
            split("s1", SplitEnd::Y, vec![], vec![]),
            make_core_block("late", true),
        ],
    );

    let err = dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain }))
        .expect_err("nothing may follow a Y split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(ids(&project.borrow().chains[0].blocks), vec!["amp"]);
}

#[test]
fn load_chain_preset_refuses_a_block_after_a_y_split() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let err = dispatcher
        .dispatch(Command::Chain(ChainCommand::LoadChainPreset {
            chain: ChainId(CHAIN.into()),
            preset_instrument: "electric_guitar".into(),
            preset_blocks: vec![
                split("s1", SplitEnd::Y, vec![], vec![]),
                make_core_block("late", true),
            ],
        }))
        .expect_err("the preset puts a block after its Y split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(ids(&project.borrow().chains[0].blocks), vec!["amp"]);
}

/// Characterization pin (green before and after): a preset whose Y split is
/// its last block loads.
#[test]
fn load_chain_preset_accepts_a_y_split_as_its_last_block() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Chain(ChainCommand::LoadChainPreset {
            chain: ChainId(CHAIN.into()),
            preset_instrument: "electric_guitar".into(),
            preset_blocks: vec![
                make_core_block("pre", true),
                split("s1", SplitEnd::Y, vec![], vec![]),
            ],
        }))
        .expect("a Y split may end a preset");

    assert_eq!(ids(&project.borrow().chains[0].blocks), vec!["pre", "s1"]);
}
