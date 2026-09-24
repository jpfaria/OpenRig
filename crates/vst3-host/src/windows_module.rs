//! Responsibility: locates the DLL of a Windows VST3 module.

use std::path::{Path, PathBuf};

/// The DLL to load for `module`, the path discovery found: a bundle directory
/// (`Foo.vst3/Contents/<arch_dir>/Foo.vst3`) or, in the layout used before
/// VST 3.6.10, the DLL itself (a `Foo.vst3` file).
pub(crate) fn windows_module_binary(module: &Path, arch_dir: &str) -> Option<PathBuf> {
    if module.is_file() {
        return Some(module.to_path_buf());
    }
    let contents = module.join("Contents").join(arch_dir);
    let named = contents.join(format!("{}.vst3", module.file_stem()?.to_string_lossy()));
    if named.is_file() {
        return Some(named);
    }
    // A renamed bundle keeps the vendor's DLL name inside; macOS and Linux
    // already fall back to any binary in the arch dir.
    std::fs::read_dir(&contents)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "vst3"))
}

/// A pre-3.6.10 module: the `.vst3` file is the DLL itself.
#[cfg(target_os = "windows")]
pub(crate) fn is_single_file_module(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|ext| ext == "vst3")
}

#[cfg(test)]
#[path = "windows_module_tests.rs"]
mod tests;
