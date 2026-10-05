//! Responsibility: wires the Settings row for the backing-tracks folder.
//!
//! The dispatcher persists the folder and moves the player's library to it,
//! so this row only asks for a folder, dispatches the command and shows the
//! choice. Both Settings roots (the inline page and the standalone window)
//! install it.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use application::command::{Command, SettingsCommand};
use infra_filesystem::FilesystemStorage;

use super::paths_apply::pick_folder_dialog;
use crate::state::ProjectSession;
use crate::SettingsBridge;

/// Connect Choose… and Reset on one root's bridge. `show` writes the label
/// back on that same root.
pub(crate) fn install(
    bridge: &SettingsBridge,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    show: impl Fn(String) + Clone + 'static,
) {
    let session = project_session.clone();
    let show_pick = show.clone();
    bridge.on_pick_backing_tracks_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        dispatch(&session, Some(path.clone()));
        show_pick(path.to_string_lossy().into_owned());
    });
    bridge.on_reset_backing_tracks_path(move || {
        dispatch(&project_session, None);
        show(String::new());
    });
}

/// The folder `config.yaml` names, empty for the default.
pub(crate) fn configured_label() -> String {
    FilesystemStorage::load_app_config()
        .unwrap_or_default()
        .paths
        .backing_tracks_path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn dispatch(project_session: &Rc<RefCell<Option<ProjectSession>>>, path: Option<PathBuf>) {
    let borrowed = project_session.borrow();
    let Some(session) = borrowed.as_ref() else {
        log::warn!("[paths] no project open; the backing-tracks folder was not changed");
        return;
    };
    if let Err(e) =
        session
            .dispatcher
            .dispatch(Command::Settings(SettingsCommand::SetBackingTracksPath {
                path,
            }))
    {
        log::warn!("[paths] Command::SetBackingTracksPath failed: {e}");
    }
}
