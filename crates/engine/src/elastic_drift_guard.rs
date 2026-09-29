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
//! #965: the level is never LEARNED above the route's cushion target plus
//! the buffer this callback is about to pop (a producer in lockstep leaves
//! exactly one such buffer on top of the resting cushion). A chain starts
//! its input stream before its output stream, and every input period before
//! the output's first callback pushes a buffer nobody pops — the ring is
//! full by the time the first window closes. Taking that first floor as the
//! level ratified the whole capacity as the route's latency for good.
//!
//! #980: nor is it ever taken BELOW the cushion a route was primed with
//! (#592) plus the popped buffer. One lucky window where the dsp-worker
//! landed just in time used to become the level, and every later trim cut
//! the primed route down to a single buffer — no margin, so each late
//! worker buffer (a VST3 enabled live) was an underrun.
//!
//! #979: nor is the level ever learned BELOW what the route needs at a
//! callback start: the fill it rests on — the cushion it was born with
//! (#592/#965 prime it with its whole resting cushion), then wherever its
//! hand-off last landed it (`elastic_hand_off`) — and never less than the
//! buffer it pops plus one buffer of slack on a route that keeps it. A window
//! whose floor dips below that saw a late or lost push — the ring empty at
//! the callback that closes the window counts no underrun yet — not a level
//! the route can rest at. Taking such a floor as the level cut the route's
//! cushion at every clean window after it: one buffer of silence each time
//! on a lean route, one buffer of cushion per late push on a deep one
//! (8 buffers under JACK, the #592 cushion off-clock), until a rebuild.
//!
//! #979 with #980: the #980 floor (the prime plus the popped buffer) holds
//! on a route that keeps no slack. A slack-keeping route's floor is the
//! `need` of its resting band instead, where its hand-off lands it: the #980
//! floor there sits one buffer above the rest whenever the prime is deeper
//! than one buffer (32 or 128 frames), and kept a stalled cycle's buffer as
//! latency for good. At the owner's 64 frames both are the same 128.
//!
//! #979: a route that keeps slack has a ring deeper than the one it had
//! before (room for a stalled output's buffer, see `ElasticBuffer`). What it
//! holds beyond that old ring plus one device buffer — the one buffer the
//! owner allowed on top — is stuck latency beyond doubt, whatever the window:
//! it is shed at once. Without it, output stalls that kept coming stacked a
//! buffer each until the ring was full, because no window closed clean. And
//! on such a route a clean window a whole buffer above the level is stuck
//! latency at any buffer size, 32 frames included.
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
    /// Lowest floor of any window without underruns, or fill the route's
    /// hand-off landed it on (#979), capped at the target plus one callback
    /// buffer; a floor below what the route needs (#979) is not a level.
    level: AtomicUsize,
    /// The route's cushion target: the most it rests at (#965).
    target: usize,
    /// Times the route shed latency.
    trims: AtomicU64,
    /// #980: the cushion the route was primed with (0 = born lean).
    rest: AtomicUsize,
    /// #979: buffers of slack the route keeps on top of the one it pops.
    slack_buffers: usize,
    /// #979: the ring a slack-keeping route had before it kept slack; one
    /// device buffer above it is the most it ever holds.
    ring_before_slack: usize,
    /// #979: the fill the route rests on — the cushion it was primed with,
    /// then wherever its hand-off last landed it.
    rest_floor: AtomicUsize,
}

impl DriftGuard {
    pub(crate) fn new(target: usize) -> Self {
        Self {
            counted: AtomicUsize::new(0),
            floor: AtomicUsize::new(UNKNOWN),
            window_underruns: AtomicU64::new(0),
            level: AtomicUsize::new(UNKNOWN),
            target,
            trims: AtomicU64::new(0),
            rest: AtomicUsize::new(0),
            slack_buffers: 0,
            ring_before_slack: 0,
            rest_floor: AtomicUsize::new(0),
        }
    }

    /// #979: the route keeps one buffer of slack on top of the one it pops —
    /// no window may teach the guard a level below that — in a ring deeper
    /// than the `ring_before` it had without it, which it never fills more
    /// than one device buffer past.
    pub(crate) fn keep_one_buffer_of_slack(&mut self, ring_before: usize) {
        self.slack_buffers = 1;
        self.ring_before_slack = ring_before;
    }

    /// #979: the route rests on `frames` — the cushion it was primed with, or
    /// where its hand-off landed it — and no window may teach the guard a
    /// level below it.
    fn rests_at(&self, frames: usize) {
        self.rest_floor.store(frames, Ordering::Relaxed);
    }

    /// #979: the route's hand-off landed it on `frames` — its rest, and so
    /// its level, which no clean window has to prove first: under
    /// disturbances from the start no window closes clean, the guard fell
    /// back to its cap, and whatever a stall left between the route's rest
    /// and that cap stayed for good.
    pub(crate) fn lands_at(&self, frames: usize) {
        self.rests_at(frames);
        self.level.fetch_min(frames, Ordering::Relaxed);
    }

    /// #979: the route shed latency outside a window (see `elastic_hand_off`).
    pub(crate) fn count_trim(&self) {
        self.trims.fetch_add(1, Ordering::Relaxed);
    }

    /// The fills a callback of `frames` may start at without being short or
    /// stuck: from what the route needs (the fill it rests on, and never less
    /// than the buffer it pops plus its slack) up to the target plus that
    /// buffer (#965). Returned as `(need, cap)`, `need <= cap`.
    #[inline]
    pub(crate) fn resting_band(&self, frames: usize) -> (usize, usize) {
        let cap = self.target.saturating_add(frames);
        let need = self
            .rest_floor
            .load(Ordering::Relaxed)
            .max(frames.saturating_mul(1 + self.slack_buffers))
            .min(cap);
        (need, cap)
    }

    /// Output callback start: `fill` frames queued, `frames` about to be
    /// popped, `underruns` the ring's running count. Returns how many frames
    /// to discard now — zero unless the route holds more than one buffer past
    /// its old ring (#979) or a window just closed above the level.
    #[inline]
    pub(crate) fn observe(&self, fill: usize, frames: usize, underruns: u64) -> usize {
        let over = self.over_the_ceiling(fill, frames);
        over + self.close_window(fill - over, frames, underruns)
    }

    /// #979: what a slack-keeping route holds beyond one device buffer past
    /// the ring it had before it kept slack — shed at once, counted.
    #[inline]
    fn over_the_ceiling(&self, fill: usize, frames: usize) -> usize {
        if self.slack_buffers == 0 {
            return 0;
        }
        let over = fill.saturating_sub(self.ring_before_slack.saturating_add(frames));
        if over > 0 {
            self.count_trim();
        }
        over
    }

    /// #953: counts `frames` into the current window and, when it closes,
    /// returns the stuck latency its floor shows above the level.
    #[inline]
    fn close_window(&self, fill: usize, frames: usize, underruns: u64) -> usize {
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
        let (need, cap) = self.resting_band(frames);
        if floor < need {
            return 0;
        }
        let level = self.held_at_the_prime(self.level.load(Ordering::Relaxed).min(cap), frames);
        if floor < level {
            self.level.store(floor, Ordering::Relaxed);
            return 0;
        }
        if floor > level + self.tolerance(frames) {
            self.trims.fetch_add(1, Ordering::Relaxed);
            return floor - level;
        }
        0
    }

    /// #980: a lucky clean window must not strip a primed route's margin — on
    /// a route that keeps no slack the level is never below its prime plus
    /// the buffer being popped. A slack-keeping route (#979) is held by its
    /// resting band's `need` instead (see the module docs).
    #[inline]
    fn held_at_the_prime(&self, level: usize, frames: usize) -> usize {
        let rest = self.rest.load(Ordering::Relaxed);
        if rest == 0 || self.slack_buffers > 0 {
            return level;
        }
        level.max(rest.saturating_add(frames))
    }

    /// Growth a clean window may show above the level before it is stuck
    /// latency. #979: on a slack-keeping route a whole buffer above the
    /// level is stuck at any buffer size — at 32 frames the 32-frame
    /// tolerance was a whole buffer, and one stalled output cycle stayed as
    /// latency for good.
    #[inline]
    fn tolerance(&self, frames: usize) -> usize {
        if self.slack_buffers == 0 {
            return SLACK_FRAMES;
        }
        SLACK_FRAMES.min(frames.saturating_sub(1))
    }

    /// #980: the route was primed with `frames` of cushion; on a route that
    /// keeps no slack it is never trimmed below that prime plus the buffer
    /// being popped. #979: it rests on that cushion — no window may teach the
    /// guard a level below it.
    pub(crate) fn hold_rest(&self, frames: usize) {
        self.rest.store(frames, Ordering::Relaxed);
        self.rests_at(frames);
    }

    /// Times stuck latency was discarded since the route was built.
    pub(crate) fn trims(&self) -> u64 {
        self.trims.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[path = "elastic_drift_guard_tests.rs"]
mod tests;
