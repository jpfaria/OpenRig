//! #561 — plugin-catalog Commands: hot-reload the whole catalog, plus
//! per-plugin load / unload, without restarting OpenRig.
//!
//! All three operations resolve the plugin roots the same way the
//! boot path does (`detect_data_root().join("plugins")` for the
//! bundled tree, `plugins_root_from_config(app_config_path)` for the
//! user tree). The registry layer is the single source of truth for
//! the mutation; this handler is just the bus-side glue.
//!
//! `SaveProject` precedent: side-effects happen here, the adapter
//! just renders the emitted event. MCP/gRPC inherit the same tool
//! surface for free via the schema-derived Command name.

use std::path::PathBuf;

use anyhow::Result;

use infra_filesystem::FilesystemStorage;

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Resolve the two plugin roots boot uses. Shared by reload /
    /// load / unload so all three flows scan the same locations.
    fn plugin_roots(&self) -> Result<Vec<PathBuf>> {
        let bundled_root: PathBuf = infra_filesystem::detect_data_root().join("plugins");
        let user_root = FilesystemStorage::app_config_path()
            .map(|cfg| plugin_loader::plugins_root_from_config(&cfg))
            .map_err(|e| anyhow::anyhow!("resolve app_config_path failed: {e}"))?;
        Ok(vec![bundled_root, user_root])
    }

    /// `Command::ReloadPluginCatalog` — re-scan the bundled + user
    /// plugin roots, rebuild the catalog, emit
    /// `Event::PluginCatalogReloaded { native_count, disk_count,
    /// total_count }`.
    pub(crate) fn handle_reload_plugin_catalog(&self) -> Result<Vec<Event>> {
        let roots = self.plugin_roots()?;
        let stats = plugin_loader::registry::reload(&roots);
        Ok(vec![Event::PluginCatalogReloaded {
            native_count: stats.native_count,
            disk_count: stats.disk_count,
            total_count: stats.total_count,
        }])
    }

    /// `Command::LoadPlugin { id }` — bring a single disk plugin
    /// into the in-memory catalog. Re-scans the known roots, adds
    /// the one whose manifest id matches `id`. Errors when no
    /// package with that id is discoverable; no-op when already
    /// loaded. Emits `Event::PluginLoaded { id }` on success.
    pub(crate) fn handle_load_plugin(&self, id: String) -> Result<Vec<Event>> {
        let roots = self.plugin_roots()?;
        plugin_loader::registry::load_one(&id, &roots).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(vec![Event::PluginLoaded { id }])
    }

    /// `Command::UnloadPlugin { id }` — remove a single disk plugin
    /// from the in-memory catalog. Refuses natives. Emits
    /// `Event::PluginUnloaded { id }` on success.
    pub(crate) fn handle_unload_plugin(&self, id: String) -> Result<Vec<Event>> {
        plugin_loader::registry::unload(&id).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(vec![Event::PluginUnloaded { id }])
    }
}
