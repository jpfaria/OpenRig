//! Responsibility: takes a runtime's processing lock from an audio thread without ever blocking it.
//!
//! The input path only ever `try_lock`s: a lost lock is a skipped buffer
//! (`input_busy_skips`), never a wait on a mutex. #987: an in-place edit now
//! holds the lock for the whole node swap (short: every fresh node is built
//! beforehand) so the audio never sees a pipeline emptied for the edit. The
//! per-input DSP worker (#670) is not the device callback — the ring and the
//! output cushion absorb a buffer that comes a little late — so it may keep
//! trying for a bounded moment instead of dropping the buffer. Between tries it
//! sleeps a few microseconds, as it already does when its ring is empty: a
//! spinning realtime worker starves the HAL and its siblings (#781). No
//! allocation.

use std::sync::{MutexGuard, TryLockError};
use std::time::{Duration, Instant};

use crate::runtime_state::{ChainProcessingState, ChainRuntimeState};

/// Pause between two tries of a patient caller.
const RETRY_PAUSE: Duration = Duration::from_micros(20);

/// The processing lock, tried once, then retried for up to `patience_ns`.
/// `patience_ns == 0` is the plain `try_lock` of the device callback.
pub(crate) fn try_lock_processing(
    runtime: &ChainRuntimeState,
    patience_ns: u64,
) -> Option<MutexGuard<'_, ChainProcessingState>> {
    let mut started: Option<Instant> = None;
    loop {
        match runtime.processing.try_lock() {
            Ok(guard) => return Some(guard),
            Err(TryLockError::Poisoned(_)) => return None,
            Err(TryLockError::WouldBlock) => {}
        }
        if patience_ns == 0 {
            return None;
        }
        let since = *started.get_or_insert_with(Instant::now);
        if since.elapsed().as_nanos() as u64 >= patience_ns {
            return None;
        }
        std::thread::sleep(RETRY_PAUSE);
    }
}

#[cfg(test)]
#[path = "runtime_processing_lock_tests.rs"]
mod runtime_processing_lock_tests;
