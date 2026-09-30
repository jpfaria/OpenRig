//! Responsibility: coalesces pending mixer-strip writes to `config.yaml`.
//! #1007 — a fader drag (GUI, or a control surface over MIDI) sends one
//! `SetMixerFader` per pointer or encoder tick. Each one used to queue its own
//! read-modify-write of `config.yaml`. Now a strip holds at most ONE queued
//! write: later values replace the pending one, and the write takes whatever
//! value is latest when it runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use infra_filesystem::MixerStripConfig;

/// Strip values waiting for their queued write, keyed by config path + id.
#[derive(Default)]
pub(crate) struct PendingStrips {
    pending: HashMap<(PathBuf, String), MixerStripConfig>,
}

impl PendingStrips {
    /// Remember `strip` as the value to write. `true` when no write is queued
    /// for it yet, so the caller has to queue one.
    pub(crate) fn submit(&mut self, path: PathBuf, strip: MixerStripConfig) -> bool {
        let key = (path, strip.id.clone());
        self.pending.insert(key, strip).is_none()
    }

    /// The latest value for the queued write, clearing it.
    pub(crate) fn take(&mut self, path: &Path, id: &str) -> Option<MixerStripConfig> {
        self.pending.remove(&(path.to_path_buf(), id.to_string()))
    }
}

/// The process-wide pending writes, shared by the dispatching thread and the
/// persist worker (never the audio thread).
pub(crate) fn pending_strips() -> MutexGuard<'static, PendingStrips> {
    static PENDING: OnceLock<Mutex<PendingStrips>> = OnceLock::new();
    PENDING
        .get_or_init(|| Mutex::new(PendingStrips::default()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
#[path = "mixer_persist_coalesce_tests.rs"]
mod tests;
