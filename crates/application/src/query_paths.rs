//! Responsibility: reports where the app resolved its paths to.

/// #582: effective resolved system paths the user-facing tooling
/// reads at runtime. Serialised as JSON for the `openrig://paths`
/// MCP resource. The struct shape — not an ad-hoc `serde_json::json!`
/// — exists so adding a new path field to [`infra_filesystem::AssetPaths`]
/// is a hard compile error here too; the envelope cannot silently drift
/// from the source of truth.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ResolvedPaths {
    /// User data root for the current install (`~/Library/Application
    /// Support/OpenRig` on macOS, `%APPDATA%\OpenRig` on Windows,
    /// `~/.local/share/openrig` on Linux). All `None` overrides
    /// resolve to a subfolder of this root.
    pub data_root: String,
    /// Effective preset directory: the override when set in
    /// `config.yaml`, otherwise `<data_root>/presets`.
    pub presets_path: String,
    /// Effective plugin directory: the override when set in
    /// `config.yaml`, otherwise `<data_root>/plugins`.
    pub plugins_path: String,
    /// Effective evaluations directory (#582): the override when set in
    /// `config.yaml`, otherwise `<data_root>/evaluations` per
    /// [`infra_filesystem::default_evaluations_path`].
    pub evaluations_path: String,
}

impl ResolvedPaths {
    /// Build a [`ResolvedPaths`] from the user-side `config.yaml`
    /// (`AppConfig.paths`). Each `None` override falls back to the OS
    /// default — consumers (skills, MCP clients) read absolute paths
    /// without re-implementing the per-platform default themselves.
    pub fn from_app_config(paths: &infra_filesystem::AssetPaths) -> Self {
        let root = infra_filesystem::user_data_root();
        let default_presets = root.join("presets");
        let default_plugins = root.join("plugins");
        let evaluations = paths
            .evaluations_path
            .clone()
            .unwrap_or_else(infra_filesystem::default_evaluations_path);
        Self {
            data_root: root.to_string_lossy().into_owned(),
            presets_path: paths
                .presets_path
                .clone()
                .unwrap_or(default_presets)
                .to_string_lossy()
                .into_owned(),
            plugins_path: paths
                .plugins_path
                .clone()
                .unwrap_or(default_plugins)
                .to_string_lossy()
                .into_owned(),
            evaluations_path: evaluations.to_string_lossy().into_owned(),
        }
    }

    /// Stable JSON wire shape for the `openrig://paths` resource. The
    /// `to_string` is infallible for this concrete struct (all fields
    /// are `String`), so the helper exposes a `String` to keep callers
    /// out of `Result` plumbing.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("ResolvedPaths serializes")
    }
}

/// #582: load the user-side `config.yaml`, resolve every override
/// (falling back to OS defaults), and serialise the
/// [`ResolvedPaths`] envelope to JSON for the `openrig://paths`
/// resource resolver in `adapter-mcp` (via the bridge query channel).
pub fn resolved_paths_json() -> String {
    let config = infra_filesystem::FilesystemStorage::load_app_config().unwrap_or_default();
    ResolvedPaths::from_app_config(&config.paths).to_json()
}
