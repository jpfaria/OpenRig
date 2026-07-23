//! Pure helpers for building and parsing the per-chain DI loop source list
//! that populates the chain-tile ComboBox (issue #614, Task 7).
//!
//! ## build_di_loop_sources(bundled_ids)
//! Returns `Vec<String>` of all bundled loop ids in order, followed by
//! [`CHOOSE_FILE_SENTINEL`] as the last entry. Slint binds this list to
//! the tile's `di-loop-sources` property.
//!
//! ## parse_di_loop_source(selected, bundled_ids)
//! Maps the string the user picked back to a [`DiLoopSource`]:
//! - A bundled id → `Some(DiLoopSource::Bundled(id))`
//! - [`CHOOSE_FILE_SENTINEL`] → `None` (caller opens the file picker)
//! - Anything else  → `None`
//!
//! Both functions are pure (no I/O, no Slint). Tested in
//! `tests/issue_614_di_loop_ui_sources.rs`.

use std::path::Path;

use application::di_loader::DiLoopSource;

/// The last entry in the ComboBox — signals "open the file picker".
pub const CHOOSE_FILE_SENTINEL: &str = "Choose file…";

/// Enumerate the bundled DI loop ids available at runtime.
///
/// Scans `<data-root>/assets/di-loops/` for `*.wav` files and returns their
/// file stems (without extension) in alphabetical order. Returns an empty
/// `Vec` when the directory does not exist or is empty (e.g. before Task 8
/// ships the first bundled loops).
///
/// Intentionally NOT called on the audio thread — only called during
/// `replace_project_chains` on the GUI thread.
pub fn bundled_di_loop_ids() -> Vec<String> {
    let root = infra_filesystem::detect_data_root();
    let dir = root.join("assets").join("di-loops");
    let Ok(read_dir) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = read_dir
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()?.eq_ignore_ascii_case("wav") {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    ids.sort();
    ids
}

/// Build the ordered source list for the chain-tile ComboBox.
///
/// Each element of `bundled_ids` becomes one entry (by its id string);
/// [`CHOOSE_FILE_SENTINEL`] is appended last.
pub fn build_di_loop_sources(bundled_ids: &[&str]) -> Vec<String> {
    let mut result: Vec<String> = bundled_ids.iter().map(|id| id.to_string()).collect();
    result.push(CHOOSE_FILE_SENTINEL.to_string());
    result
}

/// Display label for a user-chosen [`DiLoopSource::File`] in the ComboBox —
/// the file name with extension (e.g. `/x/ambience.wav` → `ambience.wav`),
/// falling back to the full path string when there is no file name (#661).
pub fn di_loop_file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .map_or_else(|| path.to_string_lossy().into_owned(), |n| n.to_string())
}

/// Build the ComboBox source list including the currently loaded source.
///
/// Bundled ids first; then, when `loaded` is a [`DiLoopSource::File`], its
/// [`di_loop_file_label`] (so the chosen file is visible and selectable);
/// then [`CHOOSE_FILE_SENTINEL`] last. A `Bundled`/`None` `loaded` adds no
/// extra entry (issue #661).
pub fn build_di_loop_sources_with_loaded(
    bundled_ids: &[&str],
    loaded: Option<&DiLoopSource>,
) -> Vec<String> {
    let mut result: Vec<String> = bundled_ids.iter().map(|id| id.to_string()).collect();
    if let Some(DiLoopSource::File(path)) = loaded {
        let label = di_loop_file_label(path);
        if !result.contains(&label) {
            result.push(label);
        }
    }
    result.push(CHOOSE_FILE_SENTINEL.to_string());
    result
}

/// Index of the currently selected `source` within the ComboBox `sources`
/// list, or `-1` when it has no row to highlight (issue #661).
///
/// A [`DiLoopSource::Bundled`] maps to the position of its id; a
/// [`DiLoopSource::File`] maps to the position of its [`di_loop_file_label`]
/// (present only when the list was built via
/// [`build_di_loop_sources_with_loaded`]). Anything not in the list yields
/// `-1`, matching Slint's `ComboBox.current-index` "nothing selected" value.
pub fn di_loop_selected_index(sources: &[String], source: &DiLoopSource) -> i32 {
    let needle = match source {
        DiLoopSource::Bundled(id) => id.clone(),
        DiLoopSource::File(path) => di_loop_file_label(path),
    };
    sources
        .iter()
        .position(|s| *s == needle)
        .map_or(-1, |pos| pos as i32)
}

/// Map a selected ComboBox entry back to a [`DiLoopSource`].
///
/// Returns `None` for the sentinel or any unknown string so the caller
/// knows to open the file picker (or ignore the selection).
pub fn parse_di_loop_source(selected: &str, bundled_ids: &[&str]) -> Option<DiLoopSource> {
    if selected == CHOOSE_FILE_SENTINEL || selected.is_empty() {
        return None;
    }
    if bundled_ids.contains(&selected) {
        return Some(DiLoopSource::Bundled(selected.to_string()));
    }
    None
}
