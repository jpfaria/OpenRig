//! Responsibility: names a recent project in the remove-confirmation dialog.
//!
//! Split out of `recent_projects_wiring` (#913). The dialog asks "remove
//! <name>?", and a project saved without a name would leave that blank — so an
//! unnamed entry falls back to the file it points at rather than to nothing.

use infra_filesystem::RecentProjectEntry;

/// The name to show for `entry`: its own name, or the file stem, or — for a
/// path with no stem at all — the raw path, which is at least something the
/// user can recognise.
pub(crate) fn confirm_removal_label(entry: &RecentProjectEntry) -> String {
    if !entry.project_name.is_empty() {
        return entry.project_name.clone();
    }
    std::path::Path::new(&entry.project_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.to_string())
        .unwrap_or_else(|| entry.project_path.clone())
}

#[cfg(test)]
#[path = "recent_project_label_tests.rs"]
mod tests;
