//! #553 — Tracks screen GUI wiring.
//!
//! Hooks the Launcher's "Tracks" button and the Tracks page's Back
//! button onto the existing `show-*` property surface. Catalog
//! scanning, separation jobs, and search queries land in follow-ups —
//! this is the minimum needed to let the user actually navigate to
//! the new screen.

use slint::ComponentHandle;

use crate::AppWindow;

/// Connect the Launcher → Tracks → Launcher navigation.
///
/// - `open-tracks-clicked` (fired from the Launcher header button)
///   hides the launcher and reveals the Tracks page.
/// - `tracks-back-clicked` (fired from the Tracks header back button)
///   restores the launcher.
pub(crate) fn wire_tracks_nav(window: &AppWindow) {
    let window_weak_open = window.as_weak();
    window.on_open_tracks_clicked(move || {
        if let Some(window) = window_weak_open.upgrade() {
            window.set_show_project_launcher(false);
            window.set_show_tracks(true);
        }
    });

    let window_weak_back = window.as_weak();
    window.on_tracks_back_clicked(move || {
        if let Some(window) = window_weak_back.upgrade() {
            window.set_show_tracks(false);
            window.set_show_project_launcher(true);
        }
    });
}
