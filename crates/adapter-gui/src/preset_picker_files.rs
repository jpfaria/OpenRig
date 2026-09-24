//! Responsibility: lists the preset files the picker offers.
//!
//! Split out of `chain_preset_wiring` (#913). The picker reads the directory
//! ONCE when it opens and filters that snapshot as the user types, so this
//! scan is what the whole list is built from: only YAML files, in a stable
//! order, each shown by a name derived from its filename.

use std::path::{Path, PathBuf};

/// The `(display name, path)` pairs under `presets_path`, sorted by filename.
///
/// A directory that cannot be read yields an empty list rather than an error:
/// the presets folder is a user setting that may point anywhere, and the picker
/// still has to open (showing "no presets") instead of failing.
pub(crate) fn scan_preset_files(presets_path: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(presets_path) else {
        return Vec::new();
    };
    let mut yaml: Vec<_> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|extension| extension == "yaml" || extension == "yml")
                .unwrap_or(false)
        })
        .collect();
    // Sorted by FILE NAME, not by whatever order the filesystem hands back —
    // the same folder must list the same way on every open and every machine.
    yaml.sort_by_key(|entry| entry.file_name());
    yaml.into_iter()
        .map(|entry| {
            let path = entry.path();
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("")
                .replace('_', " ");
            (name, path)
        })
        .collect()
}

/// The picker's list: the user's presets and the bundled ones, sorted by
/// filename, a user's file hiding a bundled one of the same name.
pub(crate) fn scan_preset_libraries(user: &Path, bundled: &Path) -> Vec<(String, PathBuf)> {
    let mut listed = scan_preset_files(user);
    if bundled != user {
        for (name, path) in scan_preset_files(bundled) {
            let file_name = path.file_name();
            if !listed.iter().any(|(_, mine)| mine.file_name() == file_name) {
                listed.push((name, path));
            }
        }
    }
    listed.sort_by(|(_, a), (_, b)| a.file_name().cmp(&b.file_name()));
    listed
}

#[cfg(test)]
#[path = "preset_picker_files_tests.rs"]
mod tests;
