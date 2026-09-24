//! Responsibility: converts between a preset name its filename.
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

/// `stem`, with a `_` after its device part when `windows` and that part is
/// one of the device names Windows reserves whatever follows (#978). Windows
/// reads the device up to the first `.`, trailing spaces dropped, so
/// "AUX.clean" is the AUX device too: it becomes "AUX_.clean".
pub fn windows_safe_stem(stem: &str, windows: bool) -> String {
    let base = stem.split('.').next().unwrap_or("").trim_end();
    if windows && is_windows_device_name(base) {
        format!("{base}_{}", &stem[base.len()..])
    } else {
        stem.to_string()
    }
}

/// CON, PRN, AUX, NUL, and COM/LPT followed by 0-9 or a superscript 1-3.
fn is_windows_device_name(base: &str) -> bool {
    let upper = base.to_uppercase();
    let port = |prefix: &str| {
        upper.strip_prefix(prefix).is_some_and(|n| {
            let mut chars = n.chars();
            matches!(
                (chars.next(), chars.next()),
                (Some('0'..='9' | '¹' | '²' | '³'), None)
            )
        })
    };
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") || port("COM") || port("LPT")
}

/// Build the on-disk filename from a user-facing preset name. Issue
/// #510: keep the exact characters the user typed; only sanitize
/// filesystem-illegal ones.
pub fn preset_filename(name: &str) -> String {
    let cleaned = sanitize_for_filename(name.trim());
    let stem = windows_safe_stem(&cleaned, cfg!(windows));
    format!("{stem}.{PRESET_EXTENSION}")
}

/// Resolve the absolute path of a preset file under the given presets
/// directory.
pub fn preset_save_path(presets_dir: &Path, name: &str) -> PathBuf {
    presets_dir.join(preset_filename(name))
}

#[cfg(test)]
#[path = "preset_file_tests.rs"]
mod tests;
