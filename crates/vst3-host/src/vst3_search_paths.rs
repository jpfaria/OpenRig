//! Responsibility: lists the directories a VST3 bundle can be installed in.

use std::path::PathBuf;

/// Returns the standard system VST3 search paths for the current platform.
///
/// Paths are returned in priority order (user-level first, system-level second).
/// None of these paths are guaranteed to exist.
pub fn system_vst3_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut paths = Vec::new();
        // User-level
        if let Some(home) = dirs_home() {
            paths.push(
                home.join("Library")
                    .join("Audio")
                    .join("Plug-Ins")
                    .join("VST3"),
            );
        }
        // System-level
        paths.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
        // Network / developer
        paths.push(PathBuf::from("/Network/Library/Audio/Plug-Ins/VST3"));
        paths
    }
    #[cfg(target_os = "windows")]
    {
        let mut paths = Vec::new();
        // %LOCALAPPDATA%\Programs\Common\VST3
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            paths.push(
                PathBuf::from(local)
                    .join("Programs")
                    .join("Common")
                    .join("VST3"),
            );
        }
        // %PROGRAMFILES%\Common Files\VST3
        if let Some(pf) = std::env::var_os("PROGRAMFILES") {
            paths.push(PathBuf::from(pf).join("Common Files").join("VST3"));
        }
        // %PROGRAMFILES(X86)%\Common Files\VST3
        if let Some(pf86) = std::env::var_os("PROGRAMFILES(X86)") {
            paths.push(PathBuf::from(pf86).join("Common Files").join("VST3"));
        }
        paths
    }
    #[cfg(target_os = "linux")]
    {
        let mut paths = Vec::new();
        // User-level
        if let Some(home) = dirs_home() {
            paths.push(home.join(".vst3"));
        }
        // System-level
        paths.push(PathBuf::from("/usr/lib/vst3"));
        paths.push(PathBuf::from("/usr/local/lib/vst3"));
        paths
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Vec::new()
    }
}

#[cfg(not(target_os = "windows"))]
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
