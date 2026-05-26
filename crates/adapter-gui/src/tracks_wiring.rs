//! #553 — Tracks screen GUI wiring.
//!
//! Hooks the Launcher's "Tracks" button and the Tracks page's Back
//! button onto the existing `show-*` property surface. Also scans the
//! tracks catalog on entry and exposes the resulting list to the
//! Slint side. Separation jobs (Import button) and the Track detail
//! view land in follow-ups — this commit is the minimum needed to
//! actually see the catalog inside the running app.

use std::path::PathBuf;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{AppWindow, TrackRowData};

/// Resolve the catalog root: `<data-dir>/OpenRig/tracks/`.
///
/// Cross-platform via `dirs::data_dir`; falls back to `./tracks` only
/// when the OS does not expose a data directory at all.
fn default_tracks_dir() -> PathBuf {
    dirs::data_dir()
        .map(|d| d.join("OpenRig").join("tracks"))
        .unwrap_or_else(|| PathBuf::from("tracks"))
}

fn format_duration(secs: f64) -> SharedString {
    if !secs.is_finite() || secs <= 0.0 {
        return "--:--".into();
    }
    let total = secs as u64;
    let m = total / 60;
    let s = total % 60;
    format!("{m}:{s:02}").into()
}

fn entry_to_row(entry: &feature_tracks::TrackEntry) -> TrackRowData {
    TrackRowData {
        id: SharedString::from(entry.meta.id.as_str()),
        title: SharedString::from(entry.meta.title.as_str()),
        artist: entry
            .meta
            .artist
            .as_deref()
            .map(SharedString::from)
            .unwrap_or_default(),
        duration: format_duration(entry.meta.duration_secs),
        stem_count: entry.meta.stems.len() as i32,
    }
}

/// Scan the tracks catalog and populate the Slint list model. Failures
/// surface as an empty model so the user sees the empty-state copy
/// rather than a frozen UI.
pub(crate) fn refresh_tracks_model(window: &AppWindow) {
    let dir = default_tracks_dir();
    let rows: Vec<TrackRowData> = feature_tracks::scan_catalog(&dir)
        .unwrap_or_default()
        .iter()
        .map(entry_to_row)
        .collect();
    let model = Rc::new(VecModel::from(rows));
    window.set_tracks(ModelRc::from(model));
}

/// Connect the Launcher → Tracks → Launcher navigation and run an
/// initial catalog scan when the Tracks button is clicked.
pub(crate) fn wire_tracks_nav(window: &AppWindow) {
    refresh_tracks_model(window);

    let window_weak_open = window.as_weak();
    window.on_open_tracks_clicked(move || {
        if let Some(window) = window_weak_open.upgrade() {
            refresh_tracks_model(&window);
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

    // Ensure the search-text property starts empty.
    window.set_tracks_search_text(SharedString::new());
}
