use std::cell::RefCell;
use std::rc::Rc;

use application::command::DrumsCommand;
use project::project::Project;

use super::{dispatch_drums, drums_panel_view, drums_snapshot, SessionCell};
use crate::state::ProjectSession;

fn closed() -> SessionCell {
    Rc::new(RefCell::new(None))
}

fn open() -> SessionCell {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-drums-session-tests"),
    ))))
}

#[test]
fn with_no_project_there_is_no_drum_machine_to_read() {
    assert!(drums_snapshot(&closed()).is_none());
    assert!(drums_panel_view(&closed()).is_none());
}

#[test]
fn with_no_project_a_command_is_dropped() {
    assert!(!dispatch_drums(
        &closed(),
        DrumsCommand::SetDrumsBpm { bpm: 120.0 }
    ));
}

#[test]
fn an_accepted_command_reaches_the_dispatchers_state() {
    let session = open();
    assert!(dispatch_drums(
        &session,
        DrumsCommand::SetDrumsBpm { bpm: 133.0 }
    ));
    assert_eq!(drums_snapshot(&session).unwrap().bpm, 133.0);
}

#[test]
fn the_panel_view_draws_the_dispatchers_state() {
    let session = open();
    dispatch_drums(&session, DrumsCommand::SetDrumsBpm { bpm: 133.0 });
    let (snapshot, view) = drums_panel_view(&session).unwrap();
    assert_eq!(snapshot.bpm, 133.0);
    assert_eq!(view.bpm, 133.0);
}

#[test]
fn a_refused_command_reports_false() {
    let session = open();
    assert!(!dispatch_drums(
        &session,
        DrumsCommand::SelectDrumKit {
            kit: "no-such-kit".into()
        }
    ));
}
