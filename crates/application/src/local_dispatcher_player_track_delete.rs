//! Responsibility: deletes a track from the user's own backing-track folder.
//!
//! Only a file inside that folder (its category subfolders included) goes;
//! the tracks that ship with the app and anything elsewhere on disk are
//! refused. A deleted track that is loaded is stopped and unloaded first, so
//! the player never points at a file that is gone. File I/O on the
//! dispatching (control) thread only.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::player_event::PlayerEvent;
use crate::player_library::is_backing_track_file;

impl LocalDispatcher {
    pub(crate) fn delete_player_track(&self, path: PathBuf) -> Result<Vec<Event>> {
        let user_dir = self.player_state().borrow().library().user;
        let target = user_track(user_dir.as_deref(), &path)?;
        let loaded = self
            .player_state()
            .borrow()
            .track()
            .and_then(|t| t.canonicalize().ok())
            .is_some_and(|t| t == target);
        let mut events = Vec::new();
        if loaded {
            if let Some(control) = self.runtime_control() {
                control.stop_player();
            }
            self.player_state().borrow_mut().clear_track();
            events.push(Event::Player(PlayerEvent::TransportChanged {
                playing: false,
            }));
        }
        std::fs::remove_file(&target)
            .with_context(|| format!("cannot delete '{}'", path.display()))?;
        events.push(Event::Player(PlayerEvent::TrackDeleted { path }));
        Ok(events)
    }
}

/// The real location of `path` when it is a backing track inside `user_dir`.
fn user_track(user_dir: Option<&Path>, path: &Path) -> Result<PathBuf> {
    let not_yours = || {
        anyhow::anyhow!(
            "'{}' is not in your backing-tracks folder; only your own tracks can be deleted",
            path.display()
        )
    };
    let dir = user_dir
        .and_then(|d| d.canonicalize().ok())
        .ok_or_else(not_yours)?;
    let target = path.canonicalize().map_err(|_| not_yours())?;
    if !target.starts_with(&dir) || !target.is_file() {
        return Err(not_yours());
    }
    if !is_backing_track_file(&target) {
        bail!("'{}' is not a backing track", path.display());
    }
    Ok(target)
}

#[cfg(test)]
#[path = "local_dispatcher_player_track_delete_tests.rs"]
mod tests;
