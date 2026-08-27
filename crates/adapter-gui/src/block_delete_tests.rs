//! #913 — deleting the block the editor is pointed at.
//!
//! The confirm dialog can outlive what it points at: the project can close, a
//! preset switch can replace the chain, another transport can remove the block
//! first. Every one of those must resolve to "nothing to delete" rather than
//! removing whatever now sits at that index.

use super::{delete_drafted_block, DeleteBlockError};
use crate::state::{BlockEditorDraft, ProjectSession};
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
        std::env::temp_dir().join("openrig-913-delete-tests"),
    ))))
}

fn draft(chain_index: usize, block_index: Option<usize>) -> BlockEditorDraft {
    BlockEditorDraft {
        chain_index,
        block_index,
        before_index: 0,
        instrument: "electric_guitar".into(),
        effect_type: "gain".into(),
        model_id: "volume".into(),
        enabled: true,
        is_select: false,
    }
}

fn rows() -> Rc<VecModel<ProjectChainItem>> {
    // Republishing the rows walks the asset paths, which panic until startup
    // set them. Defaults point at directories that do not exist in a test run.
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    Rc::new(VecModel::from(Vec::<ProjectChainItem>::new()))
}

fn block_ids(session: &Rc<RefCell<Option<ProjectSession>>>, chain: usize) -> Vec<String> {
    session.borrow().as_ref().unwrap().project.borrow().chains[chain]
        .blocks
        .iter()
        .map(|b| b.id.0.clone())
        .collect()
}

#[test]
fn the_drafted_block_is_the_one_removed() {
    let session = session(vec![chain(
        "chain:0",
        vec![block("a"), block("b"), block("c")],
    )]);

    delete_drafted_block(&session, &draft(0, Some(1)), &rows(), &[], &[]).expect("delete");

    assert_eq!(block_ids(&session, 0), vec!["a", "c"]);
}

#[test]
fn deleting_leaves_the_other_chains_alone() {
    let session = session(vec![
        chain("chain:0", vec![block("a")]),
        chain("chain:1", vec![block("b")]),
    ]);
    delete_drafted_block(&session, &draft(1, Some(0)), &rows(), &[], &[]).expect("delete");
    assert_eq!(block_ids(&session, 0), vec!["a"]);
    assert!(block_ids(&session, 1).is_empty());
}

#[test]
fn a_draft_for_a_block_being_added_has_nothing_to_delete() {
    let session = session(vec![chain("chain:0", vec![block("a")])]);
    assert_eq!(
        delete_drafted_block(&session, &draft(0, None), &rows(), &[], &[]),
        Err(DeleteBlockError::NotAnExistingBlock)
    );
    assert_eq!(block_ids(&session, 0), vec!["a"], "nothing was removed");
}

#[test]
fn a_chain_index_that_no_longer_resolves_removes_nothing() {
    let session = session(vec![chain("chain:0", vec![block("a")])]);
    assert_eq!(
        delete_drafted_block(&session, &draft(7, Some(0)), &rows(), &[], &[]),
        Err(DeleteBlockError::Gone)
    );
    assert_eq!(block_ids(&session, 0), vec!["a"]);
}

#[test]
fn a_block_index_past_the_end_removes_nothing() {
    let session = session(vec![chain("chain:0", vec![block("a")])]);
    assert_eq!(
        delete_drafted_block(&session, &draft(0, Some(9)), &rows(), &[], &[]),
        Err(DeleteBlockError::Gone),
        "another transport may have removed it first — do not delete the \
         block that shifted into that slot"
    );
    assert_eq!(block_ids(&session, 0), vec!["a"]);
}

#[test]
fn deleting_with_no_project_open_is_refused() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    assert_eq!(
        delete_drafted_block(&none, &draft(0, Some(0)), &rows(), &[], &[]),
        Err(DeleteBlockError::Gone)
    );
}

#[test]
fn deleting_republishes_the_chain_rows() {
    let session = session(vec![chain("chain:0", vec![block("a"), block("b")])]);
    let rows = rows();
    delete_drafted_block(&session, &draft(0, Some(0)), &rows, &[], &[]).expect("delete");
    use slint::Model;
    assert_eq!(
        rows.row_count(),
        1,
        "the chains screen must show the project as it is now"
    );
}

#[test]
fn deleting_the_last_block_leaves_an_empty_chain_rather_than_failing() {
    let session = session(vec![chain("chain:0", vec![block("only")])]);
    delete_drafted_block(&session, &draft(0, Some(0)), &rows(), &[], &[]).expect("delete");
    assert!(block_ids(&session, 0).is_empty());
}
