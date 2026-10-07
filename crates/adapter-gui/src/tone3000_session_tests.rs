use std::cell::RefCell;
use std::rc::Rc;

use application::command::Tone3000Command;
use project::project::Project;

use super::{dispatch_tone3000, poll_tone3000, tone3000_snapshot, SessionCell};
use crate::state::ProjectSession;

fn closed() -> SessionCell {
    Rc::new(RefCell::new(None))
}

/// An open session with no key, whatever the machine's config holds.
fn open_without_key() -> SessionCell {
    let session: SessionCell = Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-tone3000-session-tests"),
    ))));
    dispatch_tone3000(
        &session,
        Tone3000Command::SetTone3000ApiKey { key: String::new() },
    )
    .unwrap();
    session
}

fn search() -> Tone3000Command {
    Tone3000Command::SearchTone3000 {
        query: "plexi".into(),
        page: 1,
        format: None,
        gear: None,
        sort: None,
    }
}

#[test]
fn with_no_project_there_is_no_browser() {
    assert!(tone3000_snapshot(&closed()).is_none());
    assert!(poll_tone3000(&closed()).is_empty());
    assert_eq!(dispatch_tone3000(&closed(), search()), Ok(()));
}

#[test]
fn a_session_reads_the_dispatchers_state() {
    let session = open_without_key();
    let snapshot = tone3000_snapshot(&session).unwrap();
    assert!(!snapshot.key_configured);
}

#[test]
fn a_refused_command_reports_why() {
    let session = open_without_key();
    let refused = dispatch_tone3000(&session, search()).unwrap_err();
    assert!(refused.contains("Secret Key"), "{refused}");
}

#[test]
fn setting_a_key_reaches_the_dispatcher() {
    let session = open_without_key();
    dispatch_tone3000(
        &session,
        Tone3000Command::SetTone3000ApiKey {
            key: "t3k_cs_test".into(),
        },
    )
    .unwrap();
    assert!(tone3000_snapshot(&session).unwrap().key_configured);
}
