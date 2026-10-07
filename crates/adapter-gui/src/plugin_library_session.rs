//! Responsibility: reaches the user's plugin library through the project's dispatcher.
//!
//! With no project open there is no dispatcher: a read answers `None` and a
//! command is dropped, like the TONE3000 browser.

use application::command::Command;
use application::event::Event;
use application::plugin_library::entries::{library_entries, PluginLibraryEntry};
use application::query_plugin_library::{plugin_grid, PluginGridView};

use crate::tone3000_session::SessionCell;

/// Every plugin the user owns, by display name.
pub(crate) fn plugin_entries(session: &SessionCell) -> Option<Vec<PluginLibraryEntry>> {
    let roots = session
        .borrow()
        .as_ref()
        .map(|s| s.dispatcher.plugin_library_roots())?;
    Some(library_entries(&roots))
}

/// The capture grid of one owned plugin; `Err` says why it cannot be edited.
pub(crate) fn plugin_grid_of(
    session: &SessionCell,
    plugin_id: &str,
) -> Option<Result<PluginGridView, String>> {
    let roots = session
        .borrow()
        .as_ref()
        .map(|s| s.dispatcher.plugin_library_roots())?;
    Some(plugin_grid(&roots, plugin_id).map_err(|e| e.to_string()))
}

/// Send one command and hand back what it changed at once. `Err` carries
/// the refusal the window shows; no project is `Ok` with nothing changed.
pub(crate) fn dispatch_library(
    session: &SessionCell,
    command: Command,
) -> Result<Vec<Event>, String> {
    let borrowed = session.borrow();
    let Some(s) = borrowed.as_ref() else {
        return Ok(vec![]);
    };
    s.dispatcher.dispatch(command).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "plugin_library_session_tests.rs"]
mod tests;
