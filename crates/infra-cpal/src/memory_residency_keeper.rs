//! Responsibility: repeats the memory wiring for the life of the process.
//!
//! #980: the audio's memory changes while OpenRig runs — a chain rebuilt, a
//! plugin enabled live — so one pass at start-up is not enough. One ordinary
//! thread per process re-runs `memory_wiring` whenever a runtime goes live
//! ([`wire_soon`]) and every [`PASS_EVERY`] in between; a pass over regions
//! already wired only reads the kernel's region list. Waiting for the next
//! periodic pass was not enough: the kernel compressed a new chain's pages
//! within the 5 s it took, and the idle phase underran.

use std::sync::OnceLock;
use std::thread::Thread;
use std::time::{Duration, Instant};

/// Longest wait before memory nobody announced gets wired.
const PASS_EVERY: Duration = Duration::from_secs(5);

/// After a wake-up, the pass that catches what the new runtime's DSP wrote
/// once it played: a buffer allocated zeroed (a delay line) has no pages yet
/// when it goes live, and the wake-up pass skips it as untouched (#980
/// review).
const FOLLOW_UP_AFTER: Duration = Duration::from_secs(1);

/// The keeper thread, once started (`None`: the spawn failed).
static KEEPER: OnceLock<Option<Thread>> = OnceLock::new();

/// Starts the keeper once per process; later calls do nothing.
pub(crate) fn keep_resident() {
    if !cfg!(target_os = "macos") {
        return;
    }
    KEEPER.get_or_init(|| {
        let spawned = std::thread::Builder::new()
            .name("memory-residency".into())
            .spawn(|| {
                // Pending after a wake-up: when to run its follow-up pass.
                let mut follow_up: Option<Instant> = None;
                loop {
                    crate::memory_wiring::wire_private_memory();
                    let wait = follow_up.map_or(PASS_EVERY, |due| {
                        due.saturating_duration_since(Instant::now())
                    });
                    let parked = Instant::now();
                    std::thread::park_timeout(wait);
                    // Woken (a runtime went live, even during a follow-up
                    // wait): pass now, follow up once it played. Otherwise
                    // the wait ran out: that was the follow-up or the
                    // periodic pass.
                    follow_up = (parked.elapsed() < wait).then(|| Instant::now() + FOLLOW_UP_AFTER);
                }
            });
        match spawned {
            Ok(handle) => Some(handle.thread().clone()),
            Err(error) => {
                log::warn!("memory residency: keeper thread not started: {error}");
                None
            }
        }
    });
}

/// A runtime just went live: wire its memory now instead of at the next
/// periodic pass. Never blocks; does nothing before the keeper started.
pub(crate) fn wire_soon() {
    if let Some(Some(keeper)) = KEEPER.get() {
        keeper.unpark();
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "memory_residency_keeper_tests.rs"]
mod memory_residency_keeper_tests;
