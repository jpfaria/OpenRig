//! #913 — the refresh goes on the bus, project or no project.
//!
//! #829: the re-enumeration used to live inside the Slint callbacks, so
//! `RefreshAudioDevices` arriving from MCP had nothing to run. The command half
//! is here; what must hold is that a window with no project open still works
//! (the launcher's Settings screen refreshes devices before any project exists)
//! and simply reports that there was no bus to record on.

use super::dispatch_refresh;
use crate::state::ProjectSession;
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
        std::env::temp_dir().join("openrig-913-refresh-tests"),
    ))))
}

#[test]
fn with_a_project_open_the_refresh_is_recorded_on_the_bus() {
    assert!(dispatch_refresh(&session()));
}

#[test]
fn from_the_launcher_the_refresh_still_happens_with_nothing_to_record_on() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    assert!(
        !dispatch_refresh(&none),
        "no dispatcher to record on — but the caller still re-enumerates"
    );
}

#[test]
fn refreshing_repeatedly_is_accepted() {
    let session = session();
    for _ in 0..3 {
        assert!(dispatch_refresh(&session));
    }
}
