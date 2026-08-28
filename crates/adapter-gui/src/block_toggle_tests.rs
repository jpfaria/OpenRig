//! #913 — flipping a block from its row in the strip.
//!
//! Model A (#716): the chain's head input and tail output are NOT blocks — they
//! are drawn from its bindings as fixed chips — so every entry in
//! `chain.blocks` is one the user placed and the strip shows all of them,
//! including a MID port block. The row must therefore reach exactly the block
//! at that position, mid ports included.

use super::{toggle_block_at_row, ToggleBlockError};
use crate::state::ProjectSession;
use crate::ProjectChainItem;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::chain::Chain;
use project::project::Project;
use slint::VecModel;
use std::cell::RefCell;
use std::rc::Rc;

fn effect(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: Default::default(),
        }),
    }
}

fn input() -> AudioBlock {
    AudioBlock {
        id: BlockId("in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "io-main".into(),
            endpoint: "In 1".into(),
        }),
    }
}

#[allow(dead_code)]
fn output() -> AudioBlock {
    AudioBlock {
        id: BlockId("out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "io-main".into(),
            endpoint: "Out 1".into(),
        }),
    }
}

fn chain(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

fn session(chains: Vec<Chain>) -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains,
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-toggle-tests"),
    ))))
}

fn rows() -> Rc<VecModel<ProjectChainItem>> {
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    Rc::new(VecModel::from(Vec::<ProjectChainItem>::new()))
}

fn enabled_of(session: &Rc<RefCell<Option<ProjectSession>>>, block: &str) -> bool {
    session.borrow().as_ref().unwrap().project.borrow().chains[0]
        .blocks
        .iter()
        .find(|b| b.id.0 == block)
        .map(|b| b.enabled)
        .expect("block")
}

fn toggle(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: usize,
    row: usize,
) -> Result<super::ToggledBlock, ToggleBlockError> {
    toggle_block_at_row(session, chain_index, row, &rows(), &[], &[])
}

#[test]
fn each_row_reaches_the_block_at_that_position() {
    let session = session(vec![chain(
        "chain:0",
        vec![effect("gain"), effect("delay"), effect("reverb")],
    )]);

    let toggled = toggle(&session, 0, 1).expect("row 1 exists");

    assert_eq!(toggled.block_index, 1);
    assert!(!enabled_of(&session, "delay"), "the delay flipped");
    assert!(enabled_of(&session, "gain"), "its neighbours did not");
    assert!(enabled_of(&session, "reverb"));
}

#[test]
fn a_mid_port_block_is_a_row_the_user_can_toggle() {
    // #85/#716: a mid Input/Output IS a block the user placed, so it occupies
    // a row like any other — it is only the head/tail chips that are not.
    let session = session(vec![chain(
        "chain:0",
        vec![effect("gain"), input(), effect("delay")],
    )]);

    let toggled = toggle(&session, 0, 1).expect("row 1 exists");

    assert_eq!(toggled.block_index, 1);
    assert!(!enabled_of(&session, "in"));
    assert!(enabled_of(&session, "gain"));
    assert!(enabled_of(&session, "delay"));
}

#[test]
fn toggling_reports_the_state_the_block_ended_in() {
    let session = session(vec![chain("chain:0", vec![effect("gain")])]);
    assert!(!toggle(&session, 0, 0).expect("toggle").enabled);
    assert!(toggle(&session, 0, 0).expect("toggle back").enabled);
}

#[test]
fn a_chain_index_that_does_not_resolve_toggles_nothing() {
    let session = session(vec![chain("chain:0", vec![effect("gain")])]);
    assert_eq!(toggle(&session, 5, 0), Err(ToggleBlockError::NoSuchChain));
    assert!(enabled_of(&session, "gain"));
}

#[test]
fn a_row_past_the_end_toggles_nothing() {
    let session = session(vec![chain("chain:0", vec![effect("gain")])]);
    assert_eq!(toggle(&session, 0, 9), Err(ToggleBlockError::NoSuchBlock));
    assert!(enabled_of(&session, "gain"));
}

#[test]
fn toggling_with_no_project_open_is_refused() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    assert_eq!(
        toggle_block_at_row(&none, 0, 0, &rows(), &[], &[]),
        Err(ToggleBlockError::NoProject)
    );
}

#[test]
fn toggling_republishes_the_chain_rows() {
    let session = session(vec![chain("chain:0", vec![effect("gain")])]);
    let rows = rows();
    toggle_block_at_row(&session, 0, 0, &rows, &[], &[]).expect("toggle");
    use slint::Model;
    assert_eq!(rows.row_count(), 1);
}
