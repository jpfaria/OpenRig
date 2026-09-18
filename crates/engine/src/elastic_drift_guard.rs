//! Responsibility: measures the stuck latency an output route must shed.
//!
//! #953: a route whose device runs on the chain's own clock never drains a
//! cushion that grew. When its output stream stalls while the producer keeps
//! pushing, the ring sits fuller for good — extra latency that never goes
//! away, and a route that now plays behind its siblings on the same runtime.
//!
//! The guard watches the ring's LOWEST fill over a window of popped frames:
//! that floor is the route's real latency, free of per-callback jitter. The
//! lowest floor of a window without underruns is the level the route proved
//! it can hold; a later floor above it by more than the slack is latency a
//! stall left behind, and the consumer is told to discard it.
//!
//! Consumer-only state (the output callback): `Relaxed` atomics, no lock, no
//! allocation (invariant #8).

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Popped frames per observation window (~186 ms at 44.1 kHz).
pub(crate) const WINDOW_FRAMES: usize = 8192;
/// Growth tolerated above the proven level before it counts as stuck latency.
pub(crate) const SLACK_FRAMES: usize = 32;

const UNKNOWN: usize = usize::MAX;

pub(crate) struct DriftGuard {
    /// Frames popped in the current window.
    counted: AtomicUsize,
    /// Lowest fill seen at a callback start in the current window.
    floor: AtomicUsize,
    /// Underrun count when the current window started.
    window_underruns: AtomicU64,
    /// Lowest floor of any window without underruns.
    level: AtomicUsize,
    /// Times the guard asked to discard.
    trims: AtomicU64,
}

impl DriftGuard {
    pub(crate) fn new() -> Self {
        Self {
            counted: AtomicUsize::new(0),
            floor: AtomicUsize::new(UNKNOWN),
            window_underruns: AtomicU64::new(0),
            level: AtomicUsize::new(UNKNOWN),
            trims: AtomicU64::new(0),
        }
    }

    /// Output callback start: `fill` frames queued, `frames` about to be
    /// popped, `underruns` the ring's running count. Returns how many frames
    /// to discard now — zero unless a window just closed above the level.
    #[inline]
    pub(crate) fn observe(&self, fill: usize, frames: usize, underruns: u64) -> usize {
        let floor = self.floor.load(Ordering::Relaxed).min(fill);
        let counted = self.counted.load(Ordering::Relaxed) + frames;
        if counted < WINDOW_FRAMES {
            self.floor.store(floor, Ordering::Relaxed);
            self.counted.store(counted, Ordering::Relaxed);
            return 0;
        }
        self.floor.store(UNKNOWN, Ordering::Relaxed);
        self.counted.store(0, Ordering::Relaxed);
        let clean = self.window_underruns.swap(underruns, Ordering::Relaxed) == underruns;
        if !clean {
            return 0;
        }
        let level = self.level.load(Ordering::Relaxed);
        if floor < level {
            self.level.store(floor, Ordering::Relaxed);
            return 0;
        }
        if floor > level + SLACK_FRAMES {
            self.trims.fetch_add(1, Ordering::Relaxed);
            return floor - level;
        }
        0
    }

    /// Times stuck latency was discarded since the route was built.
    pub(crate) fn trims(&self) -> u64 {
        self.trims.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[path = "elastic_drift_guard_tests.rs"]
mod tests;
