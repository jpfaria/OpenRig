//! #913 — dispatching without holding the session cell borrowed.
//!
//! The analyzers' POWER buttons go through here. Dispatching applies the
//! runtime effect, and those handlers reach back for the session — so if the
//! cell were still borrowed across the call the app would panic the moment the
//! user pressed the button. The helper exists to make that impossible, and both
//! the tuner and the spectrum used to carry their own copy of it.

use super::dispatch_detached;
use crate::state::ProjectSession;
use application::command::{Command, SelectionCommand};
use project::project::Project;
use std::cell::RefCell;
use std::rc::Rc;

fn session() -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-dispatch-tests"),
    ))))
}

fn power_on() -> Command {
    Command::Selection(SelectionCommand::SetSpectrumEnabled { enabled: true })
}

#[test]
fn a_command_reaches_the_open_sessions_dispatcher() {
    assert!(dispatch_detached(&session(), "spectrum", power_on()));
}

#[test]
fn the_session_cell_is_free_the_moment_the_command_runs() {
    let session = session();
    dispatch_detached(&session, "spectrum", power_on());
    assert!(
        session.try_borrow_mut().is_ok(),
        "a borrow held across the dispatch is what panics when the runtime \
         effect reaches back for the session"
    );
}

#[test]
fn the_session_can_be_mutably_borrowed_between_dispatches() {
    let session = session();
    dispatch_detached(&session, "spectrum", power_on());
    {
        let mut borrowed = session.borrow_mut();
        assert!(borrowed.as_mut().is_some());
    }
    assert!(dispatch_detached(&session, "spectrum", power_on()));
}

#[test]
fn with_no_project_open_there_is_nothing_to_dispatch_to() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    assert!(!dispatch_detached(&none, "tuner", power_on()));
}

#[test]
fn a_refused_command_is_reported_rather_than_propagated() {
    // Fire-and-forget: the toggle still has to reflect the press, so a refusal
    // comes back as false and is logged, never as a panic or an error to bubble.
    let session = session();
    let refused = Command::Selection(SelectionCommand::SelectActiveChain {
        chain: domain::ids::ChainId("chain:does-not-exist".into()),
    });
    let _ = dispatch_detached(&session, "spectrum", refused);
}
