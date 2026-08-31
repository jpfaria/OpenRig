//! #913 — removing one entry from the launcher's recent list.
//!
//! The confirmation dialog can outlive the list it was raised on: another
//! transport may have removed the entry, or an open may have reordered it. A
//! stale index must therefore drop NOTHING — taking out the neighbour is the
//! failure that loses a project the user still wanted.

use super::remove_recent;
use crate::state::ProjectSession;
use crate::RecentProjectItem;
use infra_filesystem::{AppConfig, RecentProjectEntry};
use project::project::Project;
use slint::{Model, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

fn entry(name: &str) -> RecentProjectEntry {
    RecentProjectEntry {
        project_path: format!("/p/{name}.yaml"),
        project_name: name.to_string(),
        is_valid: true,
        invalid_reason: None,
    }
}

fn config(names: &[&str]) -> Rc<RefCell<AppConfig>> {
    let mut config = AppConfig::default();
    config.recent_projects = names.iter().map(|n| entry(n)).collect();
    Rc::new(RefCell::new(config))
}

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
        std::env::temp_dir().join("openrig-913-recent-remove-tests"),
    ))))
}

fn rows() -> Rc<VecModel<RecentProjectItem>> {
    Rc::new(VecModel::from(Vec::<RecentProjectItem>::new()))
}

fn names(config: &Rc<RefCell<AppConfig>>) -> Vec<String> {
    config
        .borrow()
        .recent_projects
        .iter()
        .map(|e| e.project_name.clone())
        .collect()
}

#[test]
fn the_confirmed_entry_is_the_one_removed() {
    let config = config(&["a", "b", "c"]);
    assert!(remove_recent(&session(), &config, &rows(), 1, ""));
    assert_eq!(names(&config), vec!["a", "c"]);
}

#[test]
fn removing_republishes_the_list() {
    let config = config(&["a", "b"]);
    let rows = rows();
    remove_recent(&session(), &config, &rows, 0, "");
    assert_eq!(rows.row_count(), 1);
}

#[test]
fn the_republished_list_keeps_the_current_search() {
    let config = config(&["lead", "clean", "lead rhythm"]);
    let rows = rows();
    remove_recent(&session(), &config, &rows, 1, "lead");
    assert_eq!(
        rows.row_count(),
        2,
        "the launcher must not jump back to the unfiltered list"
    );
}

#[test]
fn a_stale_index_removes_nothing() {
    let config = config(&["a", "b"]);
    assert!(!remove_recent(&session(), &config, &rows(), 7, ""));
    assert_eq!(
        names(&config),
        vec!["a", "b"],
        "taking out the neighbour is how a project the user wanted disappears"
    );
}

#[test]
fn removing_from_the_launcher_with_no_project_open_still_works() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    let config = config(&["a"]);
    assert!(remove_recent(&none, &config, &rows(), 0, ""));
    assert!(names(&config).is_empty());
}

#[test]
fn removing_the_last_entry_leaves_an_empty_list() {
    let config = config(&["only"]);
    let rows = rows();
    remove_recent(&session(), &config, &rows, 0, "");
    assert!(names(&config).is_empty());
    assert_eq!(rows.row_count(), 0);
}
