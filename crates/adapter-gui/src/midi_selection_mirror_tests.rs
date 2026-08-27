//! #913 — the footswitch sees what the user has selected.
//!
//! #548/#591: the MIDI daemon runs on its own thread and cannot read the
//! `!Send` dispatcher, so the drain tick copies the selection across. Without
//! that copy a footswitch bound to "toggle the active chain" acts on whatever
//! was selected when the app started — which is what the user experiences as a
//! pedal stuck on the wrong chain.

use super::mirror_selection;
use crate::chain_selection::select_chain;
use crate::state::ProjectSession;
use application::SelectionState;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

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
        std::env::temp_dir().join("openrig-913-midi-mirror-tests"),
    ))))
}

fn daemon() -> Arc<RwLock<SelectionState>> {
    Arc::new(RwLock::new(SelectionState::default()))
}

#[test]
fn the_daemon_snapshot_takes_the_dispatchers_active_chain() {
    let session = session(vec![chain("chain:0"), chain("chain:1")]);
    {
        let borrowed = session.borrow();
        select_chain(borrowed.as_ref().unwrap(), 1).expect("select");
    }
    let daemon = daemon();

    assert!(mirror_selection(&session, &daemon));

    assert_eq!(
        daemon.read().expect("read").active_chain.as_deref(),
        Some("chain:1"),
        "the pedal must act on the chain the player is looking at"
    );
}

#[test]
fn a_later_selection_replaces_the_snapshot() {
    let session = session(vec![chain("chain:0"), chain("chain:1")]);
    let daemon = daemon();
    {
        let borrowed = session.borrow();
        select_chain(borrowed.as_ref().unwrap(), 0).expect("select");
    }
    mirror_selection(&session, &daemon);
    {
        let borrowed = session.borrow();
        select_chain(borrowed.as_ref().unwrap(), 1).expect("select");
    }
    mirror_selection(&session, &daemon);
    assert_eq!(
        daemon.read().expect("read").active_chain.as_deref(),
        Some("chain:1"),
        "the snapshot must not stick to the first selection of the session"
    );
}

#[test]
fn with_no_project_open_there_is_nothing_to_mirror() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    let daemon = daemon();
    assert!(!mirror_selection(&none, &daemon));
    assert!(daemon.read().expect("read").active_chain.is_none());
}

#[test]
fn mirroring_with_nothing_selected_yet_is_still_a_refresh() {
    // The tick runs from startup; an untouched project simply mirrors "nothing
    // selected", which is what the daemon must see rather than a stale value.
    let session = session(vec![chain("chain:0")]);
    let daemon = daemon();
    assert!(mirror_selection(&session, &daemon));
    assert!(daemon.read().expect("read").active_chain.is_none());
}

#[test]
fn mirroring_repeatedly_is_idempotent() {
    let session = session(vec![chain("chain:0")]);
    {
        let borrowed = session.borrow();
        select_chain(borrowed.as_ref().unwrap(), 0).expect("select");
    }
    let daemon = daemon();
    for _ in 0..3 {
        assert!(mirror_selection(&session, &daemon));
    }
    assert_eq!(
        daemon.read().expect("read").active_chain.as_deref(),
        Some("chain:0")
    );
}
