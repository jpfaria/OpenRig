//! Responsibility: resolves the directories the app reads its bundled assets from.
//!
//! Split out of `lib.rs` (#873). The paths differ per platform and per
//! packaging — a macOS `.app`, a Linux package, a dev checkout — so the
//! resolution lives in one place and the result is published once through a
//! process-wide `OnceLock`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Central configuration for asset directories used by the engine and GUI.
///
/// Each field holds a path (absolute or relative to the executable) where
/// the corresponding asset category lives. When the app starts it loads
/// these values from `config.yaml` (falling back to sensible defaults) and
/// stores them in a global `OnceLock` so every crate can access them
/// without passing config around.
///
/// Plugin assets — NAM/IR captures, LV2 binaries and metadata — moved to
/// the OpenRig-plugins repo in issue #287 and are resolved via
/// [`plugin_loader::config::plugins_root_from_config`], NOT through this
/// struct. Only UI-side asset categories live here now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPaths {
    /// Root directory for block thumbnails (PNG images).
    #[serde(default = "default_thumbnails")]
    pub thumbnails: String,
    /// Root directory for block screenshots (PNG images for info panel).
    #[serde(default = "default_screenshots")]
    pub screenshots: String,
    /// Root directory for plugin metadata YAML files (per-language).
    #[serde(default = "default_metadata")]
    pub metadata: String,
    /// #513: user-chosen directory holding project preset libraries. `None`
    /// keeps the historical OS default (the launcher resolves it). When set,
    /// this override wins for preset discovery / save dialogs.
    #[serde(default)]
    pub presets_path: Option<PathBuf>,
    /// #513: user-chosen directory holding plugin packs (NAM/IR/LV2). `None`
    /// keeps the historical OS default resolved by
    /// `plugin_loader::config::plugins_root_from_config`. When set, this
    /// override wins for plugin scanning.
    #[serde(default)]
    pub plugins_path: Option<PathBuf>,
    /// #582: user-chosen directory where tone analyzers and other tools
    /// write evaluation artifacts (spectrograms, fingerprints, comparison
    /// reports). `None` keeps the OS default resolved by
    /// [`default_evaluations_path`]. Machine-local concern per ADR 0003 —
    /// lives in `config.yaml`, not the project YAML.
    #[serde(default)]
    pub evaluations_path: Option<PathBuf>,
}

impl Default for AssetPaths {
    fn default() -> Self {
        Self {
            thumbnails: default_thumbnails(),
            screenshots: default_screenshots(),
            metadata: default_metadata(),
            presets_path: None,
            plugins_path: None,
            evaluations_path: None,
        }
    }
}

fn default_thumbnails() -> String {
    "assets/blocks/thumbnails".to_string()
}

fn default_screenshots() -> String {
    "assets/blocks/screenshots".to_string()
}

fn default_metadata() -> String {
    "assets/blocks/metadata".to_string()
}

static ASSET_PATHS: OnceLock<AssetPaths> = OnceLock::new();

/// Detect the application data root for the current installation layout.
///
/// Returns the directory that contains `libs/`, `data/`, and `assets/`:
///
/// - macOS `.app` bundle: `<bundle>/Contents/Resources/`
/// - Linux deb/rpm: `/usr/share/openrig/`
/// - Windows MSI: directory alongside the executable
/// - Development fallback: current working directory
pub fn detect_data_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        #[cfg(target_os = "macos")]
        if let Some(resources) = exe
            .parent() // .app/Contents/MacOS/
            .and_then(|p| p.parent()) // .app/Contents/
            .map(|p| p.join("Resources"))
        {
            if resources.exists() {
                return resources;
            }
        }

        #[cfg(target_os = "linux")]
        if let Some(exe_dir) = exe.parent() {
            if let Some(prefix) = exe_dir.parent() {
                let share = prefix.join("share/openrig");
                if share.exists() {
                    return share;
                }
            }
        }

        #[cfg(target_os = "windows")]
        if let Some(exe_dir) = exe.parent() {
            if exe_dir.join("libs").exists() {
                return exe_dir.to_path_buf();
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Resolve relative asset paths against the detected data root.
///
/// Absolute paths in `paths` are left unchanged. Relative paths are joined
/// with `detect_data_root()` so the app finds its assets regardless of the
/// current working directory.
pub fn resolve_asset_paths(paths: AssetPaths) -> AssetPaths {
    let root = detect_data_root();
    fn resolve(root: &std::path::Path, s: String) -> String {
        let p = std::path::Path::new(&s);
        if p.is_absolute() {
            s
        } else {
            root.join(p).to_string_lossy().into_owned()
        }
    }
    AssetPaths {
        thumbnails: resolve(&root, paths.thumbnails),
        screenshots: resolve(&root, paths.screenshots),
        metadata: resolve(&root, paths.metadata),
        // #513: user overrides are stored absolute (file picker resolves them).
        // No data-root rebase — `None` means "use the OS default" and is the
        // signal the resolvers look for. Same applies to #582's
        // `evaluations_path`.
        presets_path: paths.presets_path,
        plugins_path: paths.plugins_path,
        evaluations_path: paths.evaluations_path,
    }
}

/// #582: OS default for the evaluations directory (tone analyzer outputs,
/// fingerprint snapshots, A/B comparison reports). Per CLAUDE.md
/// cross-platform rule:
///
/// - macOS: `~/Library/Application Support/OpenRig/evaluations/`
/// - Windows: `%APPDATA%\OpenRig\evaluations\`
/// - Linux: `~/.local/share/openrig/evaluations/`
///
/// Used when [`AssetPaths::evaluations_path`] is `None`. Returns the path
/// without creating it — callers materialize the directory only when they
/// actually write into it.
pub fn default_evaluations_path() -> PathBuf {
    user_data_root().join("evaluations")
}

/// #582: OS-specific user data root for OpenRig
/// (`~/Library/Application Support/OpenRig` on macOS,
/// `%APPDATA%\OpenRig` on Windows,
/// `~/.local/share/openrig` on Linux). Mirrors the same convention
/// `FilesystemStorage::app_config_path` uses, kept as a shared helper so
/// every `default_*_path` derived from it stays consistent.
pub fn user_data_root() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join("Library/Application Support/OpenRig")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        appdata.join("OpenRig")
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".local/share/openrig")
    }
}

/// Store the resolved asset paths for the lifetime of the process.
///
/// Must be called once during app startup (after loading config).  Subsequent
/// calls are silently ignored so that tests that initialise multiple times do
/// not panic.
pub fn init_asset_paths(paths: AssetPaths) {
    ASSET_PATHS.set(paths).ok();
}

/// Retrieve the global asset paths.
///
/// # Panics
/// Panics if `init_asset_paths` has not been called yet.
pub fn asset_paths() -> &'static AssetPaths {
    ASSET_PATHS
        .get()
        .expect("asset_paths not initialized — call init_asset_paths() during startup")
}
