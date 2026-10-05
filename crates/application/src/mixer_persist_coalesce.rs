//! Responsibility: coalesces pending mixer-strip writes to `config.yaml`.
//! #1007 — a fader drag (GUI, or a control surface over MIDI) sends one
//! `SetMixerFader` per pointer or encoder tick. Each one used to queue its own
//! read-modify-write of `config.yaml`. Now a strip holds at most ONE queued
//! write: later values replace the pending one, and the write runs only once
//! the strip has been quiet for the settle time, carrying the drop value.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use infra_filesystem::MixerStripConfig;

/// What the queued write of one strip should do now.
#[derive(Debug, PartialEq)]
pub(crate) enum Settle {
    /// The strip went quiet: write this value.
    Ready(MixerStripConfig),
    /// The strip moved recently: check again after this long.
    Wait(Duration),
    /// Nothing pending (already written).
    Gone,
}

/// Strip values waiting for their queued write, keyed by config path + id,
/// with the instant of the latest move.
#[derive(Default)]
pub(crate) struct PendingStrips {
    pending: HashMap<(PathBuf, String), (MixerStripConfig, Instant)>,
}

impl PendingStrips {
    /// Remember `strip` as the value to write. `true` when no write is queued
    /// for it yet, so the caller has to queue one.
    pub(crate) fn submit(&mut self, path: PathBuf, strip: MixerStripConfig) -> bool {
        self.submit_at(path, strip, Instant::now())
    }

    /// `submit`, stamping the move at `now`.
    pub(crate) fn submit_at(
        &mut self,
        path: PathBuf,
        strip: MixerStripConfig,
        now: Instant,
    ) -> bool {
        let key = (path, strip.id.clone());
        self.pending.insert(key, (strip, now)).is_none()
    }

    /// The value to write once the strip has not moved for `settle`, clearing
    /// it; otherwise how long is left before it will have.
    pub(crate) fn take_settled(
        &mut self,
        path: &Path,
        id: &str,
        now: Instant,
        settle: Duration,
    ) -> Settle {
        let key = (path.to_path_buf(), id.to_string());
        let Some((_, moved_at)) = self.pending.get(&key) else {
            return Settle::Gone;
        };
        let quiet = now.saturating_duration_since(*moved_at);
        if quiet < settle {
            return Settle::Wait(settle - quiet);
        }
        self.pending
            .remove(&key)
            .map_or(Settle::Gone, |(strip, _)| Settle::Ready(strip))
    }

    /// The latest value for the queued write, clearing it.
    #[cfg(test)]
    pub(crate) fn take(&mut self, path: &Path, id: &str) -> Option<MixerStripConfig> {
        self.pending
            .remove(&(path.to_path_buf(), id.to_string()))
            .map(|(strip, _)| strip)
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
