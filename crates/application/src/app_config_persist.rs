//! Dispatch-time-bound `config.yaml` writes on the persist worker (#731).
//!
//! Persisting `AppConfig` runs off the dispatching thread on the single
//! `persist_worker`. The destination MUST be resolved here, at dispatch
//! time, and handed to the worker — never re-resolved inside the worker
//! closure. Otherwise a HOME-swap test (the `with_tmp_home` pattern) that
//! restores the real `$HOME` before the queued write lands makes the
//! worker write the fixtures onto the user's real `~/Library/Application
//! Support/OpenRig/config.yaml` (recurrence of #701 — "settings vanish on
//! every open"). Binding the path here makes the write target the `$HOME`
//! that was active at dispatch, regardless of later swaps.

use infra_filesystem::{AppConfig, FilesystemStorage};

/// Read-modify-write `config.yaml` on the persist worker, against the path
/// bound NOW. `mutate` runs on the worker thread after the current config
/// is loaded from that same fixed path.
pub fn persist_app_config(mutate: impl FnOnce(&mut AppConfig) + Send + 'static) {
    let config_path = FilesystemStorage::app_config_path();
    crate::persist_worker::run(move || {
        let config_path = match config_path {
            Ok(path) => path,
            Err(e) => {
                log::error!("persist app config: resolve config path failed: {e}");
                return;
            }
        };
        if let Err(e) = FilesystemStorage::update_app_config_at(&config_path, mutate) {
            log::error!("persist app config failed: {e}");
        }
    });
}

/// Write a fully-formed `AppConfig` snapshot to `config.yaml` on the
/// persist worker, against the path bound NOW. Use when the caller already
/// holds the complete desired config (e.g. recents sync).
pub fn persist_app_config_snapshot(snapshot: AppConfig) {
    let config_path = FilesystemStorage::app_config_path();
    crate::persist_worker::run(move || {
        let config_path = match config_path {
            Ok(path) => path,
            Err(e) => {
                log::error!("persist app config snapshot: resolve config path failed: {e}");
                return;
            }
        };
        if let Err(e) = FilesystemStorage::save_app_config_at(&config_path, &snapshot) {
            log::error!("persist app config snapshot failed: {e}");
        }
    });
}
