//! Filename ↔ preset-name helpers (#555 round 2 of #436).
//!
//! These used to live in `adapter-gui::chain_preset_wiring` next to the
//! UI callback that called them. That made the dispatcher unable to
//! resolve the on-disk path for `Command::DeleteChainPreset` /
//! `Command::SaveChainPreset`, so the GUI had to do the `fs::remove_file`
//! / `fs::write` itself — a violation of "backend transport-agnostic" /
//! "GUI sem regra de negócio" laws.
//!
//! Moved here so the dispatcher and the GUI share one source of truth.

use std::path::{Path, PathBuf};

/// On-disk extension for preset library files. Single source of truth.
pub const PRESET_EXTENSION: &str = "yaml";

/// Replace filesystem-illegal characters with `_`. The user-visible name
/// is preserved otherwise — no lowercasing, no whitespace substitution
/// (issue #510 feedback).
pub fn sanitize_for_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            _ => c,
        })
        .collect()
}

/// Build the on-disk filename from a user-facing preset name. Issue
/// #510: keep the exact characters the user typed; only sanitize
/// filesystem-illegal ones.
pub fn preset_filename(name: &str) -> String {
    let cleaned = sanitize_for_filename(name.trim());
    format!("{cleaned}.{PRESET_EXTENSION}")
}

/// Resolve the absolute path of a preset file under the given presets
/// directory.
pub fn preset_save_path(presets_dir: &Path, name: &str) -> PathBuf {
    presets_dir.join(preset_filename(name))
}

#[cfg(test)]
#[path = "preset_file_tests.rs"]
mod tests;
