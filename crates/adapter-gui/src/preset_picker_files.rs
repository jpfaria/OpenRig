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

#[cfg(test)]
#[path = "preset_picker_files_tests.rs"]
mod tests;
