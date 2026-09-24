//! Responsibility: picks the base directory the app's config lives under on Windows.

use std::ffi::OsString;
use std::path::PathBuf;

/// `%APPDATA%` when it is set, else the Roaming known folder.
pub(crate) fn windows_config_base(
    appdata: Option<OsString>,
    known_folder: Option<PathBuf>,
) -> Option<PathBuf> {
    appdata
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or(known_folder)
}

#[cfg(test)]
#[path = "config_base_tests.rs"]
mod tests;
