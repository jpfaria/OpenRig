//! #913 — dragging a block to a new slot in the chain.
//!
//! The position dispatched is the one AFTER the block is lifted out, which is
//! off by one from the drop target whenever the block moves rightwards. A chain
//! is a signal path, so landing one slot short of where the user let go is an
//! audible difference, not a cosmetic one.

use super::{reorder_block, ReorderBlockError};
use crate::state::ProjectSession;
use crate::ProjectChainItem;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::project::Project;
use slint::VecModel;
use std::cell::RefCell;
use std::rc::Rc;

fn block(id: &str) -> AudioBlock {
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

fn chain(ids: &[&str]) -> Chain {
    Chain {
        id: ChainId("chain:0".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: ids.iter().map(|id| block(id)).collect(),
        di_output: None,
        loopers: vec![],
    }
}

fn session(chain: Chain) -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![chain],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-reorder-tests"),
    ))))
}

fn rows() -> Rc<VecModel<ProjectChainItem>> {
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    Rc::new(VecModel::from(Vec::<ProjectChainItem>::new()))
}

fn order(session: &Rc<RefCell<Option<ProjectSession>>>) -> Vec<String> {
    session.borrow().as_ref().unwrap().project.borrow().chains[0]
        .blocks
        .iter()
        .map(|b| b.id.0.clone())
        .collect()
}

fn drag(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    from: usize,
    before: usize,
) -> Result<ChainId, ReorderBlockError> {
    reorder_block(session, 0, from, before, &rows(), &[], &[])
}

#[test]
fn dragging_a_block_to_the_front_puts_it_first() {
    let session = session(chain(&["a", "b", "c"]));
    drag(&session, 2, 0).expect("move c before a");
    assert_eq!(order(&session), vec!["c", "a", "b"]);
}

#[test]
fn dragging_a_block_rightwards_lands_where_the_user_let_go() {
    let session = session(chain(&["a", "b", "c", "d"]));
    // Lift 'a' and drop it before 'd': once 'a' is out, 'd' sits at index 2.
    drag(&session, 0, 3).expect("move a before d");
    assert_eq!(
        order(&session),
        vec!["b", "c", "a", "d"],
        "off by one here would leave 'a' before 'c' instead"
    );
}

#[test]
fn dragging_to_the_end_puts_it_last() {
    let session = session(chain(&["a", "b", "c"]));
    drag(&session, 0, 3).expect("move a past the end");
    assert_eq!(order(&session), vec!["b", "c", "a"]);
}

#[test]
fn dropping_a_block_on_itself_changes_nothing() {
    let session = session(chain(&["a", "b", "c"]));
    assert_eq!(drag(&session, 1, 1), Err(ReorderBlockError::NoMove));
    assert_eq!(order(&session), vec!["a", "b", "c"]);
}

#[test]
fn dropping_a_block_into_the_gap_it_already_occupies_changes_nothing() {
    let session = session(chain(&["a", "b", "c"]));
    assert_eq!(
        drag(&session, 1, 2),
        Err(ReorderBlockError::NoMove),
        "'before the next one' is where it already is"
    );
    assert_eq!(order(&session), vec!["a", "b", "c"]);
}

#[test]
fn dragging_from_a_row_that_does_not_exist_changes_nothing() {
    let session = session(chain(&["a", "b"]));
    assert_eq!(drag(&session, 9, 0), Err(ReorderBlockError::NoMove));
    assert_eq!(order(&session), vec!["a", "b"]);
}

#[test]
fn a_chain_index_that_does_not_resolve_changes_nothing() {
    let session = session(chain(&["a", "b"]));
    assert_eq!(
        reorder_block(&session, 7, 0, 1, &rows(), &[], &[]),
        Err(ReorderBlockError::NoSuchChain)
    );
}

#[test]
fn reordering_with_no_project_open_is_refused() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    assert_eq!(
        reorder_block(&none, 0, 0, 1, &rows(), &[], &[]),
        Err(ReorderBlockError::NoProject)
    );
}

#[test]
fn a_two_block_chain_can_be_swapped_both_ways() {
    let session = session(chain(&["a", "b"]));
    drag(&session, 1, 0).expect("b before a");
    assert_eq!(order(&session), vec!["b", "a"]);
    drag(&session, 1, 0).expect("a before b");
    assert_eq!(order(&session), vec!["a", "b"]);
}

#[test]
fn reordering_republishes_the_chain_rows() {
    let session = session(chain(&["a", "b"]));
    let rows = rows();
    reorder_block(&session, 0, 1, 0, &rows, &[], &[]).expect("move");
    use slint::Model;
    assert_eq!(rows.row_count(), 1);
}
