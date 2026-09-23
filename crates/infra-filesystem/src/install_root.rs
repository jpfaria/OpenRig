//! Responsibility: recognises a Windows install directory next to the executable.

use std::path::{Path, PathBuf};

/// The directory holding the executable, when it is an installed OpenRig
/// (the MSI or the portable zip): the package stages `assets\` next to
/// `openrig.exe`. `None` for a dev build, whose exe sits in `target\<profile>`.
pub(crate) fn windows_install_root(exe_dir: &Path) -> Option<PathBuf> {
    exe_dir
        .join("assets")
        .is_dir()
        .then(|| exe_dir.to_path_buf())
}

#[cfg(test)]
#[path = "install_root_tests.rs"]
mod tests;
