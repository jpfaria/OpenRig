//! #913 — what closing a project must clear.
//!
//! The launcher is reachable at any time, so this runs with a project open and
//! without one. Whichever, afterwards the session must be gone, the saved
//! snapshot forgotten (or a later "is it dirty?" compares against a project
//! that is no longer loaded) and the chain rows emptied.

use super::close_session;
use crate::state::ProjectSession;
use crate::ProjectChainItem;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;
use slint::{Model, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn open_session(chains: Vec<Chain>) -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: Some("Studio".into()),
            device_settings: vec![],
            chains,
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-close-tests"),
    ))))
}

fn rows(n: usize) -> Rc<VecModel<ProjectChainItem>> {
    Rc::new(VecModel::from(
        (0..n)
            .map(|_| ProjectChainItem::default())
            .collect::<Vec<_>>(),
    ))
}

#[test]
fn closing_an_open_project_drops_the_session() {
    let session = open_session(vec![chain("chain:0")]);
    let chains = rows(1);
    let snapshot = Rc::new(RefCell::new(Some("saved yaml".to_string())));

    close_session(&session, &chains, &snapshot, &[], &[]);

    assert!(
        session.borrow().is_none(),
        "a project that is closed while its session lives keeps answering as open"
    );
}

#[test]
fn closing_forgets_the_saved_snapshot() {
    let session = open_session(vec![chain("chain:0")]);
    let chains = rows(1);
    let snapshot = Rc::new(RefCell::new(Some("saved yaml".to_string())));
    close_session(&session, &chains, &snapshot, &[], &[]);
    assert!(
        snapshot.borrow().is_none(),
        "a stale snapshot would make the next dirty check compare against a \
         project nobody has open"
    );
}

#[test]
fn closing_empties_the_chain_rows() {
    let session = open_session(vec![chain("chain:0"), chain("chain:1")]);
    let chains = rows(2);
    let snapshot = Rc::new(RefCell::new(None));
    close_session(&session, &chains, &snapshot, &[], &[]);
    assert_eq!(
        chains.row_count(),
        0,
        "the launcher must not show the rows of a project that is gone"
    );
}

#[test]
fn closing_with_no_project_open_is_a_no_op() {
    let session: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    let chains = rows(0);
    let snapshot = Rc::new(RefCell::new(None));
    close_session(&session, &chains, &snapshot, &[], &[]);
    assert!(session.borrow().is_none());
    assert_eq!(chains.row_count(), 0);
}

#[test]
fn closing_twice_is_safe() {
    let session = open_session(vec![chain("chain:0")]);
    let chains = rows(1);
    let snapshot = Rc::new(RefCell::new(Some("saved".to_string())));
    close_session(&session, &chains, &snapshot, &[], &[]);
    close_session(&session, &chains, &snapshot, &[], &[]);
    assert!(session.borrow().is_none());
}
