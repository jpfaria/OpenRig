//! Responsibility: decides which output frames a host-clock-timed playback owes.
//!
//! A looper playback is timed so every loop on the shared timeline sounds at
//! the same loop position: it stays silent until its start instant, lands its
//! first frame on the exact output frame of that instant, and when it is late
//! (or the ring ran dry) it owes the frames it missed, which the callback pays
//! by dropping them. Only the output callback touches the mutable state.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::loop_sync::frames_between;

pub(crate) struct PlaybackTiming {
    /// Host-clock instant (ns) of the playback's first frame; 0 = untimed.
    start_at_ns: u64,
    rate: u32,
    loop_len: u64,
    started: AtomicBool,
    /// Frames the playback is behind the timeline, modulo the loop length.
    debt: AtomicU64,
}

impl PlaybackTiming {
    /// A playback that starts on the first callback and owes nothing.
    pub(crate) fn untimed(loop_len: usize) -> Self {
        Self::at(0, 1, loop_len)
    }

    /// A playback whose first frame is due at `start_at_ns`.
    pub(crate) fn at(start_at_ns: u64, rate: u32, loop_len: usize) -> Self {
        Self {
            start_at_ns,
            rate: rate.max(1),
            loop_len: loop_len.max(1) as u64,
            started: AtomicBool::new(false),
            debt: AtomicU64::new(0),
        }
    }

    pub(crate) fn is_timed(&self) -> bool {
        self.start_at_ns != 0
    }

    /// For a buffer of `frames` heard from `playback_ns`: `None` while the
    /// start is beyond it, otherwise how many leading frames stay silent.
    /// Starting late records the missed frames as debt.
    pub(crate) fn begin(&self, playback_ns: Option<u64>, frames: usize) -> Option<usize> {
        if self.started.load(Ordering::Relaxed) || !self.is_timed() {
            self.started.store(true, Ordering::Relaxed);
            return Some(0);
        }
        let Some(now) = playback_ns else {
            self.started.store(true, Ordering::Relaxed);
            return Some(0);
        };
        let skip = frames_between(now, self.start_at_ns, self.rate);
        if skip >= frames as u64 {
            return None;
        }
        self.owe(frames_between(self.start_at_ns, now, self.rate));
        self.started.store(true, Ordering::Relaxed);
        Some(skip as usize)
    }

    /// Take every frame owed so far, leaving none.
    pub(crate) fn take_debt(&self) -> u64 {
        self.debt.swap(0, Ordering::Relaxed)
    }

    /// Owe `frames` more frames; a whole loop owed is no debt at all.
    pub(crate) fn owe(&self, frames: u64) {
        if frames == 0 || !self.is_timed() {
            return;
        }
        let debt = self.debt.load(Ordering::Relaxed);
        self.debt
            .store((debt + frames) % self.loop_len, Ordering::Relaxed);
    }
}
