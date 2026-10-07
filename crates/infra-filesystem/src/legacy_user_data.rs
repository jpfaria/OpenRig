//! Responsibility: copies the user's files from the folders older releases used into the user folder.
//!
//! Older releases kept the config in the OS config folder and the rest in the
//! OS data folder. The copy only adds what the user folder lacks: it never
//! overwrites, never deletes the old folder, and runs once.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::user_data_root;

/// Written into the user folder once the copy ran, so it never runs twice.
pub const MIGRATION_MARKER: &str = ".legacy-copied";

/// Folders older releases kept the user's files in, on this OS.
pub fn legacy_user_data_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        Vec::new()
    }
    #[cfg(target_os = "macos")]
    {
        home()
            .map(|home| vec![home.join("Library/Application Support/OpenRig")])
            .unwrap_or_default()
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let mut roots = Vec::new();
        if let Some(home) = home() {
            roots.push(home.join(".local/share/openrig"));
        }
        if let Some(config) = dirs::config_dir() {
            roots.push(config.join("OpenRig"));
        }
        roots
    }
}

#[cfg(not(target_os = "windows"))]
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Bring this user's files from the old folders into the user folder. A
/// failure is logged, never fatal: the app still starts on the new folder.
pub fn copy_legacy_user_data() {
    let root = user_data_root();
    if let Err(error) = copy_legacy_into(&root, &legacy_user_data_roots()) {
        log::warn!(
            "could not copy the old user files into {}: {error}",
            root.display()
        );
    }
}

/// Copy into `root` every entry of each folder in `legacy_roots` that `root`
/// does not hold yet, then mark `root` so the copy never runs again.
pub fn copy_legacy_into(root: &Path, legacy_roots: &[PathBuf]) -> io::Result<()> {
    let marker = root.join(MIGRATION_MARKER);
    if marker.exists() {
        return Ok(());
    }
    fs::create_dir_all(root)?;
    for legacy in legacy_roots {
        if !legacy.is_dir() || legacy == root {
            continue;
        }
        for entry in fs::read_dir(legacy)? {
            let entry = entry?;
            let target = root.join(entry.file_name());
            if target.symlink_metadata().is_err() {
                copy_entry(&entry.path(), &target)?;
            }
        }
    }
    fs::write(marker, b"")
}

fn copy_entry(from: &Path, to: &Path) -> io::Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_entry(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}
