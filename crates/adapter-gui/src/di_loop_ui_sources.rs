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
            if path.extension()?.to_ascii_lowercase() == "wav" {
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

/// Map a selected ComboBox entry back to a [`DiLoopSource`].
///
/// Returns `None` for the sentinel or any unknown string so the caller
/// knows to open the file picker (or ignore the selection).
pub fn parse_di_loop_source(selected: &str, bundled_ids: &[&str]) -> Option<DiLoopSource> {
    if selected == CHOOSE_FILE_SENTINEL || selected.is_empty() {
        return None;
    }
    if bundled_ids.iter().any(|id| *id == selected) {
        return Some(DiLoopSource::Bundled(selected.to_string()));
    }
    None
}
