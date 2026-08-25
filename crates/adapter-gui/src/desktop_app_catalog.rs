//! Responsibility: loads the plugin catalog the app starts with.
//!
//! Two roots are scanned: the bundled one (read-only, ships with the
//! installer, and wins when the same package id exists in both) and the
//! user-installed one next to the GUI config file. Native (compiled-in)
//! plugins register first and disk-package discovery pushes its results into
//! the same catalog, so by the time `packages()` is read everything lives in
//! one place. VST3 system paths are scanned on a background thread — startup
//! is not blocked and the catalog is ready before any project is opened.

use crate::state::ProjectPaths;

pub(crate) fn load(project_paths: &ProjectPaths, vst3_sample_rate: f64) {
    let bundled_root = infra_filesystem::detect_data_root().join("plugins");
    let user_root = plugin_loader::plugins_root_from_config(&project_paths.default_config_path);
    log::info!(
        "scanning plugin roots: bundled={} user={}",
        bundled_root.display(),
        user_root.display(),
    );
    engine::native_registry::register_all_natives();
    plugin_loader::registry::init_many(&[bundled_root.clone(), user_root.clone()]);
    log::info!(
        "plugin catalog ready: {} plugin(s) loaded ({} native, {} disk package(s))",
        plugin_loader::registry::len(),
        plugin_loader::registry::native_count(),
        plugin_loader::registry::len() - plugin_loader::registry::native_count(),
    );
    std::thread::spawn(move || {
        project::vst3_editor::init_vst3_catalog(vst3_sample_rate, &[bundled_root, user_root]);
    });
}
