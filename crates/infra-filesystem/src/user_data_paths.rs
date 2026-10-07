//! Responsibility: resolves where the user's own files live on this machine.
//!
//! One folder per user holds everything OpenRig writes for them: the system
//! config at its root, the presets, the projects, the looper takes, the
//! evaluations. Never hardcoded — derived from the OS home directory.

use std::path::PathBuf;

/// The user's OpenRig folder: `~/.openrig` on macOS and Linux,
/// `%APPDATA%\OpenRig` on Windows. Every `default_*_path` derives from it.
pub fn user_data_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        // `%APPDATA%` first: the Known Folder API ignores the environment, so
        // nothing (a test included) could point the folder elsewhere (#978).
        crate::config_base::windows_config_base(std::env::var_os("APPDATA"), dirs::config_dir())
            .unwrap_or_else(|| PathBuf::from("."))
            .join("OpenRig")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".openrig")
    }
}

/// Default folder of the user's presets (`<user folder>/presets`).
pub fn default_presets_path() -> PathBuf {
    user_data_root().join("presets")
}

/// Default folder new projects are saved in (`<user folder>/projects`).
pub fn default_projects_path() -> PathBuf {
    user_data_root().join("projects")
}

/// Default folder of the evaluations (tone analyzer outputs, fingerprint
/// snapshots, A/B comparison reports), used when
/// [`crate::AssetPaths::evaluations_path`] is `None`. Returned without creating
/// it — callers materialize the directory only when they write into it.
pub fn default_evaluations_path() -> PathBuf {
    user_data_root().join("evaluations")
}

/// Default folder of the app-wide library of saved looper takes. Every
/// project sees it; the DI source picker lists it. Returned without creating it.
pub fn default_looper_takes_path() -> PathBuf {
    user_data_root().join("looper-takes")
}

/// Default folder of the user's own backing tracks. Returned without creating it.
pub fn default_backing_tracks_path() -> PathBuf {
    user_data_root().join("backing-tracks")
}
