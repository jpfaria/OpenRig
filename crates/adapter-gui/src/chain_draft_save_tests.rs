//! #913 — committing the chain the editor was drafting.
//!
//! Both editors (the inline one and the detached window) save through here, so
//! the rules hold for either: a chain with no binding selected is refused
//! rather than saved unopenable (#716), editing replaces the chain in place
//! instead of appending a copy, and the chain rows are republished so the
//! screen shows what was just saved.

use super::{save_drafted_chain, SaveChainError};
use crate::state::{ChainDraft, ProjectSession};
use crate::ProjectChainItem;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;
use slint::{Model, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

fn chain(id: &str, description: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some(description.to_string()),
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec!["io-main".into()],
        blocks: vec![],
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
        std::env::temp_dir().join("openrig-913-chain-save-tests"),
    ))))
}

fn draft(editing_index: Option<usize>, name: &str, bindings: Vec<String>) -> ChainDraft {
    ChainDraft {
        editing_index,
        name: name.to_string(),
        instrument: "electric_guitar".to_string(),
        io_binding_ids: bindings,
    }
}

fn rows() -> Rc<VecModel<ProjectChainItem>> {
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    Rc::new(VecModel::from(Vec::<ProjectChainItem>::new()))
}

fn chain_count(session: &Rc<RefCell<Option<ProjectSession>>>) -> usize {
    session
        .borrow()
        .as_ref()
        .unwrap()
        .project
        .borrow()
        .chains
        .len()
}

#[test]
fn a_draft_with_no_binding_selected_is_refused() {
    let session = session(vec![]);
    assert_eq!(
        save_drafted_chain(&session, &draft(None, "Guitar", vec![]), &rows(), &[], &[]),
        Err(SaveChainError::NoBindingSelected),
        "#716: a chain with no binding has no I/O to open"
    );
    assert_eq!(chain_count(&session), 0, "nothing was saved");
}

#[test]
fn a_new_draft_appends_a_chain() {
    let session = session(vec![]);
    save_drafted_chain(
        &session,
        &draft(None, "Guitar", vec!["io-main".into()]),
        &rows(),
        &[],
        &[],
    )
    .expect("save");
    assert_eq!(chain_count(&session), 1);
}

#[test]
fn editing_replaces_the_chain_in_place_instead_of_appending_a_copy() {
    let session = session(vec![chain("chain:0", "Guitar"), chain("chain:1", "Bass")]);
    save_drafted_chain(
        &session,
        &draft(Some(0), "Lead Guitar", vec!["io-main".into()]),
        &rows(),
        &[],
        &[],
    )
    .expect("save");
    assert_eq!(chain_count(&session), 2, "no copy was appended");
    assert_eq!(
        session.borrow().as_ref().unwrap().project.borrow().chains[0]
            .description
            .as_deref(),
        Some("Lead Guitar")
    );
}

#[test]
fn editing_keeps_the_chains_identity_so_the_runtime_can_be_resynced() {
    let session = session(vec![chain("chain:0", "Guitar")]);
    let saved = save_drafted_chain(
        &session,
        &draft(Some(0), "Lead Guitar", vec!["io-main".into()]),
        &rows(),
        &[],
        &[],
    )
    .expect("save");
    assert_eq!(
        saved,
        ChainId("chain:0".into()),
        "a new id would leave the live runtime pointing at a chain nobody edits"
    );
}

#[test]
fn saving_republishes_the_chain_rows() {
    let session = session(vec![chain("chain:0", "Guitar")]);
    let rows = rows();
    save_drafted_chain(
        &session,
        &draft(Some(0), "Lead Guitar", vec!["io-main".into()]),
        &rows,
        &[],
        &[],
    )
    .expect("save");
    assert_eq!(rows.row_count(), 1, "the screen shows what was saved");
}

#[test]
fn saving_with_no_project_open_is_refused_rather_than_silently_dropped() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    let result = save_drafted_chain(
        &none,
        &draft(None, "Guitar", vec!["io-main".into()]),
        &rows(),
        &[],
        &[],
    );
    assert!(matches!(result, Err(SaveChainError::Failed(_))));
}

#[test]
fn an_editing_index_that_no_longer_resolves_saves_as_a_new_chain() {
    // The editor can outlive the chain it was opened on (a preset switch, a
    // removal from another transport). Saving must not panic on the stale
    // index; it lands as a new chain the user can see and remove.
    let session = session(vec![chain("chain:0", "Guitar")]);
    save_drafted_chain(
        &session,
        &draft(Some(9), "Ghost", vec!["io-main".into()]),
        &rows(),
        &[],
        &[],
    )
    .expect("save");
    assert_eq!(chain_count(&session), 2);
}
