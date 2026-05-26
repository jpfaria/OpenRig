//! Red-first (#436 sweep): `Command::RegisterRecentProject` /
//! `MarkRecentProjectInvalid` despacham e emitem
//! `Event::RecentProjectRegistered` / `RecentProjectInvalidated`.
//! Precedente `SaveProject` (persistência no adapter).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use project::project::Project;

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

fn dispatcher() -> LocalDispatcher {
    LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })))
}

#[test]
fn register_recent_project_emits_event() {
    let events = dispatcher()
        .dispatch(Command::RegisterRecentProject {
            path: PathBuf::from("/p/a.yaml"),
            name: "A".to_string(),
        })
        .expect("RegisterRecentProject deve ok");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::RecentProjectRegistered { path, name }
                if path == &PathBuf::from("/p/a.yaml") && name == "A"
        )),
        "esperava Event::RecentProjectRegistered, veio {events:?}"
    );
}

#[test]
fn mark_recent_project_invalid_emits_event() {
    let events = dispatcher()
        .dispatch(Command::MarkRecentProjectInvalid {
            path: PathBuf::from("/p/b.yaml"),
            reason: "gone".to_string(),
        })
        .expect("MarkRecentProjectInvalid deve ok");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::RecentProjectInvalidated { path, reason }
                if path == &PathBuf::from("/p/b.yaml") && reason == "gone"
        )),
        "esperava Event::RecentProjectInvalidated, veio {events:?}"
    );
}
