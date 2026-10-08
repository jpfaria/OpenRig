use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, PluginLibraryCommand};
use project::project::Project;

use super::{dispatch_library, plugin_entries, plugin_grid_of};
use crate::state::ProjectSession;
use crate::tone3000_session::SessionCell;

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
        std::env::temp_dir().join("openrig-plugin-library-session-tests"),
    ))))
}

fn uninstall(id: &str) -> Command {
    Command::PluginLibrary(PluginLibraryCommand::UninstallPlugin {
        plugin_id: id.into(),
    })
}

#[test]
fn with_no_project_there_is_no_library() {
    assert!(plugin_entries(&closed()).is_none());
    assert!(plugin_grid_of(&closed(), "any").is_none());
    assert_eq!(dispatch_library(&closed(), uninstall("any")), Ok(vec![]));
}

#[test]
fn an_open_project_lists_the_library() {
    assert!(plugin_entries(&open()).is_some());
}

#[test]
fn a_plugin_outside_the_catalog_has_no_grid() {
    let refused = plugin_grid_of(&open(), "no_such_plugin")
        .unwrap()
        .unwrap_err();
    assert!(refused.contains("no_such_plugin"), "{refused}");
}

#[test]
fn a_refused_command_reports_why() {
    let refused = dispatch_library(&open(), uninstall("no_such_plugin")).unwrap_err();
    assert!(!refused.is_empty());
}
