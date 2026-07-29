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

use std::path::PathBuf;

use infra_filesystem::{AppConfig, FilesystemStorage, MetronomeConfig};

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

/// #14: read-modify-write only the metronome section of `config.yaml` on the
/// persist worker. `config_path` is the per-machine SYSTEM config (ADR 0003);
/// `None` resolves the OS path once, HERE, never inside the worker.
///
/// Every metronome write funnels through this one door, and `MetronomeConfig`
/// has no `enabled` field — so "the app always starts with the metronome off"
/// holds structurally instead of depending on each call site remembering it.
pub fn persist_metronome(
    config_path: Option<PathBuf>,
    mutate: impl FnOnce(&mut MetronomeConfig) + Send + 'static,
) {
    let config_path = config_path
        .map(Ok)
        .unwrap_or_else(FilesystemStorage::app_config_path);
    crate::persist_worker::run(move || {
        let config_path = match config_path {
            Ok(path) => path,
            Err(e) => {
                log::error!("persist metronome: resolve config path failed: {e}");
                return;
            }
        };
        if let Err(e) = FilesystemStorage::update_app_config_at(&config_path, |config| {
            mutate(&mut config.metronome)
        }) {
            log::error!("persist metronome failed: {e}");
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
