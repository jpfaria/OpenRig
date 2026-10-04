//! Responsibility: restores the backing-track player's state into a session's dispatcher.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use application::dispatcher::CommandDispatcher;
use application::player_library::PlayerLibraryDirs;
use application::player_state::PlayerControlState;
use infra_filesystem::{
    bundled_backing_tracks_path, default_backing_tracks_path, FilesystemStorage,
};

/// Gives the dispatcher the player state every transport shares: the volume
/// and output restored from the machine's `config.yaml`, the two library
/// folders, and the file the state persists back to.
pub(crate) fn attach_player_state(dispatcher: &dyn CommandDispatcher) {
    let config = FilesystemStorage::load_app_config().unwrap_or_default();
    let library = PlayerLibraryDirs {
        bundled: Some(bundled_backing_tracks_path()),
        user: Some(
            config
                .paths
                .backing_tracks_path
                .clone()
                .unwrap_or_else(default_backing_tracks_path),
        ),
    };
    dispatcher.attach_player_state(Rc::new(RefCell::new(PlayerControlState::restored(
        &config.player,
        library,
        player_config_path(),
    ))));
}

/// `None` in a test build, so no test can rewrite the real `config.yaml`.
#[cfg(not(test))]
fn player_config_path() -> Option<PathBuf> {
    FilesystemStorage::app_config_path().ok()
}

#[cfg(test)]
fn player_config_path() -> Option<PathBuf> {
    None
}
