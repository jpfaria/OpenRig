//! `LocalDispatcher` system paths command handler (issue #792 split).
//!
//! Single responsibility: the system-level path overrides (presets, plugins,
//! evaluations — ADR 0003 system settings). The command owns the persistence
//! (write into `config.yaml` on the persist worker) then emits `PathsSaved`.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// #513 / #540: system-level paths overrides (presets, plugins).
    /// The command owns the persistence: write the picked path into
    /// `config.yaml` (ADR 0003 — system setting), then emit
    /// `Event::PathsSaved` so listeners (GUI label refresh, MCP, gRPC)
    /// can pick up the change without re-reading from disk.
    ///
    /// The previous handler (#513) emitted the event only and relied on
    /// "the adapter persists on PathsSaved" — but the event carries no
    /// path payload and no listener was wired, so the user's pick
    /// survived only in memory and reset to default on the next launch
    /// (issue #540).
    pub(crate) fn handle_paths_system(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            // #693: path persistence runs on the persist worker — the
            // dispatching thread never waits on disk; errors go to the
            // non-blocking logger.
            // #731: bind the config path at dispatch time (see app_config_persist);
            // the worker must not re-resolve `$HOME` at write time.
            Command::SetPresetsPath { path } => {
                crate::app_config_persist::persist_app_config(move |config| {
                    config.paths.presets_path = path;
                });
                Ok(vec![Event::PathsSaved])
            }
            Command::SetPluginsPath { path } => {
                crate::app_config_persist::persist_app_config(move |config| {
                    config.paths.plugins_path = path;
                });
                Ok(vec![Event::PathsSaved])
            }
            Command::SetEvaluationsPath { path } => {
                crate::app_config_persist::persist_app_config(move |config| {
                    config.paths.evaluations_path = path;
                });
                Ok(vec![Event::PathsSaved])
            }
            other => {
                unreachable!("handle_paths_system received non-paths command: {other:?}")
            }
        }
    }
}
