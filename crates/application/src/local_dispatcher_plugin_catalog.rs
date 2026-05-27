//! #561 — `Command::ReloadPluginCatalog`: hot-reload the plugin catalog
//! without restarting OpenRig.
//!
//! Resolves the plugin roots the same way the boot path does
//! (`detect_data_root().join("plugins")` for the bundled tree,
//! `plugins_root_from_config(app_config_path)` for the user tree),
//! calls [`plugin_loader::registry::reload`] to rebuild the catalog,
//! and emits `Event::PluginCatalogReloaded` with the new totals.
//!
//! `SaveProject` precedent: side-effects (re-scanning disk, swapping
//! the registry slice) happen here, the adapter just renders the
//! emitted event. MCP/gRPC inherit the same tool surface for free via
//! the schema-derived Command name.

use std::path::PathBuf;

use anyhow::Result;

use infra_filesystem::FilesystemStorage;

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `Command::ReloadPluginCatalog` — re-scan the bundled + user
    /// plugin roots, rebuild the catalog, emit
    /// `Event::PluginCatalogReloaded { native_count, disk_count,
    /// total_count }`.
    ///
    /// Errors out cleanly if the app config path cannot be resolved
    /// (`FilesystemStorage::app_config_path` failed) — we'd otherwise
    /// have no way to compute the user-installed plugins root.
    pub(crate) fn handle_reload_plugin_catalog(&self) -> Result<Vec<Event>> {
        let bundled_root: PathBuf = infra_filesystem::detect_data_root().join("plugins");
        let user_root = FilesystemStorage::app_config_path()
            .map(|cfg| plugin_loader::plugins_root_from_config(&cfg))
            .map_err(|e| anyhow::anyhow!("resolve app_config_path failed: {e}"))?;
        let stats = plugin_loader::registry::reload(&[bundled_root, user_root]);
        Ok(vec![Event::PluginCatalogReloaded {
            native_count: stats.native_count,
            disk_count: stats.disk_count,
            total_count: stats.total_count,
        }])
    }
}
