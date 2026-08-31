//! #913 — renaming the open project.
//!
//! Two fields edit the same name (the main window's and the settings window's)
//! and both go through here, so the project cannot end up named one thing on
//! one surface and another elsewhere. The name reaches the project itself, not
//! just the widget — a rename that stopped at the GUI would be lost on save.

use super::record_project_name;
use crate::state::ProjectSession;
use project::project::Project;
use std::cell::RefCell;
use std::rc::Rc;

fn session(name: Option<&str>) -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: name.map(str::to_string),
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-name-tests"),
    ))))
}

fn current_name(session: &Rc<RefCell<Option<ProjectSession>>>) -> Option<String> {
    session.borrow().as_ref()?.project.borrow().name.clone()
}

#[test]
fn the_typed_name_reaches_the_project() {
    let session = session(Some("Untitled"));
    assert!(record_project_name(&session, "Studio Rig"));
    assert_eq!(current_name(&session).as_deref(), Some("Studio Rig"));
}

#[test]
fn the_last_keystroke_wins() {
    let session = session(None);
    for typed in ["S", "St", "Stu", "Studio"] {
        record_project_name(&session, typed);
    }
    assert_eq!(current_name(&session).as_deref(), Some("Studio"));
}

#[test]
fn clearing_the_field_unsets_the_name_rather_than_saving_an_empty_one() {
    let session = session(Some("Studio"));
    assert!(record_project_name(&session, ""));
    assert_eq!(
        current_name(&session),
        None,
        "an empty name means the project has none, so the launcher falls back \
         to the filename instead of showing a blank row"
    );
}

#[test]
fn typing_with_no_project_open_renames_nothing() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    assert!(
        !record_project_name(&none, "Studio"),
        "the settings screen is reachable from the launcher"
    );
}
