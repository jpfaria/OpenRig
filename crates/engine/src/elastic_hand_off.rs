//! Responsibility: lands an output route on its rest when its producer's hand-off starts or resumes.
//!
//! #979: on macOS the chain DSP runs on the #670 worker, and the worker hands
//! a cycle's buffer to the route AFTER that cycle's output callbacks (#965
//! measured every output unit 0–5 µs after the input). A route primed with
//! exactly its cushion therefore met its first callback holding only that
//! cushion: it popped it whole, and from then on every callback started with
//! nothing queued behind the buffer it pops. One worker period late was one
//! buffer of silence.
//!
//! A route that keeps one buffer of slack rests inside its resting band
//! (`DriftGuard::resting_band`): at a callback start it holds at least the
//! buffer it pops plus one buffer, or its own cushion when that is deeper
//! (`need`), and at most its target plus the buffer it pops (`cap`, #965). It
//! lands there whenever the producer's hand-off resumes: at the first callback
//! that finds the producer's first buffer, and at the first callback that
//! finds it again after the ring ran dry.
//!
//! - **The first hand-off** — until it lands the route plays what the ring
//!   holds (its prime). The callback that finds it lands the route: short, it
//!   plays silence and pops nothing, once, and the next hand-off lands on top
//!   — one device buffer deeper, exactly one; long (its output stream came up
//!   after the input had run ahead), it drops everything queued above `need`
//!   before anything was heard, which the #953 guard would otherwise cut
//!   ~186 ms in. It lands on `need`, the floor of its band, never higher: a
//!   route that kept what queued up to `cap` rested one buffer later than the
//!   same route whose stream came up with the input (at 32 frames: 96 frames
//!   per callback start instead of 64), and two outputs of one guitar played
//!   the same note a buffer apart. Landing at the route's first callback
//!   instead, before the producer had handed anything over, left a route
//!   whose output stream came up before the input — by even one cycle —
//!   resting on the bare hand-off, with no slack, for good. A producer that
//!   pushes before the output callback (inline DSP, a DI render) lands on
//!   `need` the same way.
//! - **After a gap** — a worker late by L periods left the ring dry for L-1
//!   callbacks, then handed everything over at once: the ring holds the
//!   route's rest plus the silence the gap played. It sheds exactly that
//!   silence, never below `need`, crossfaded and counted as a latency trim,
//!   on the callback the late buffers land. It used to keep it as latency
//!   until a #953 window passed clean — never, while lateness kept coming —
//!   so each late worker left the route another buffer later. Short after a
//!   gap, it waits one callback, counted with the gap as underrun frames.
//!
//! Where the route lands is its rest: the drift guard learns it as its level
//! (`DriftGuard::lands_at`) and never needs a clean window first.
//!
//! #979: a route that keeps NO slack but runs on its producer's own clock (a
//! route no convolver feeds, an insert send) lands once, at its producer's
//! first hand-off, on the buffer it pops (`need`) — never waiting, so it
//! never rests deeper than a lockstep start rests it — and never again. Such
//! a route is born empty, and every input period before its output stream's
//! first callback left a buffer in its ring that it kept as latency for the
//! life of the chain: a stream up two HAL cycles after the input rested at
//! 128 frames per callback start next to a lockstep sibling at 64 — the first
//! live capture's route 0 at fill 64 and route 2 at 128, both on Main. The
//! #953 guard never shed it: the cap it allows (#965) is the target plus the
//! buffer it pops. A route on another clock does not land: its producer hands
//! over bursts of another device's size at no fixed point of its cycle.
//!
//! Consumer-only state (the output callback): `Relaxed` atomics, no lock, no
//! allocation (invariant #8).

use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};

/// What a route's output callback does before it pops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Landing {
    /// Pop as usual.
    Play,
    /// Play silence and pop nothing: the hand-off lands on the route's
    /// cushion. `in_a_gap`: the route had been playing and its ring ran dry —
    /// this silence is part of that gap and is counted with it.
    Wait { in_a_gap: bool },
    /// The route lands here: discard `excess` frames first, then play; it
    /// rests on what remains. `heard`: the excess is the silence a gap played,
    /// shed as a counted latency trim; otherwise it queued before anything was
    /// heard.
    Land { excess: usize, heard: bool },
}

/// No landing pending.
const SETTLED: u8 = 0;
/// The route waits for the producer's hand-off: its first one, or the first
/// after its ring ran dry.
const PENDING: u8 = 1;
/// The hand-off came, the route was short and waited one callback: it lands
/// at the next one, on whatever it finds.
const WAITED: u8 = 2;
/// The route never lands, or never again.
const OFF: u8 = 3;

/// `owed` before the route's first landing: nothing queued was heard.
const UNHEARD: usize = usize::MAX;

pub(crate) struct HandOffLanding {
    state: AtomicU8,
    /// #979: the route keeps no slack — it lands at its producer's first
    /// hand-off only, never waits for it, and never lands again.
    first_only: bool,
    /// Silence the route played since its ring ran dry: the latency the late
    /// hand-off brings back on top of its rest. `UNHEARD` before the first
    /// landing.
    owed: AtomicUsize,
    /// The ring's underrun count at the previous callback start.
    seen_underruns: AtomicU64,
    /// Before the first landing: what the ring would hold at this callback
    /// start had the producer handed nothing over — the prime, less what the
    /// callbacks since popped. More than that is the first hand-off.
    unfed: AtomicUsize,
}

impl HandOffLanding {
    /// A route that never lands (its level is someone else's, or its
    /// producer runs on another clock).
    pub(crate) fn off() -> Self {
        Self::in_state(OFF, 0, false)
    }

    /// A slack-keeping route: lands on its rest at its producer's first
    /// hand-off and after every gap.
    pub(crate) fn armed() -> Self {
        Self::in_state(PENDING, UNHEARD, false)
    }

    /// A route that keeps no slack, on its producer's clock: lands once, at
    /// its producer's first hand-off, on the buffer it pops.
    pub(crate) fn at_first_hand_off() -> Self {
        Self::in_state(PENDING, UNHEARD, true)
    }

    fn in_state(state: u8, owed: usize, first_only: bool) -> Self {
        Self {
            state: AtomicU8::new(state),
            first_only,
            owed: AtomicUsize::new(owed),
            seen_underruns: AtomicU64::new(0),
            unfed: AtomicUsize::new(0),
        }
    }

    /// Build time: the route was primed with `frames` of silence.
    pub(crate) fn primed_with(&self, frames: usize) {
        self.unfed.store(frames, Ordering::Relaxed);
    }

    /// Output callback start: `fill` frames queued, `frames` about to be
    /// popped, the route rests on `need` frames at a callback start (the
    /// floor of its resting band), and the ring has played `underruns` silent
    /// frames since it was built.
    #[inline]
    pub(crate) fn step(
        &self,
        fill: usize,
        frames: usize,
        (need, cap): (usize, usize),
        underruns: u64,
    ) -> Landing {
        let mut state = self.state.load(Ordering::Relaxed);
        if state == OFF {
            return Landing::Play;
        }
        let seen = self.seen_underruns.swap(underruns, Ordering::Relaxed);
        let gap = usize::try_from(underruns.saturating_sub(seen)).unwrap_or(usize::MAX);
        if gap > 0 {
            let owed = if state == SETTLED {
                gap
            } else {
                self.owed.load(Ordering::Relaxed).saturating_add(gap)
            };
            self.owed.store(owed, Ordering::Relaxed);
            if state == SETTLED {
                state = PENDING;
                self.state.store(PENDING, Ordering::Relaxed);
            }
        }
        if state == SETTLED {
            return Landing::Play;
        }
        let owed = self.owed.load(Ordering::Relaxed);
        if owed == UNHEARD {
            return self.first_hand_off(fill, frames, (need, cap), state);
        }
        if fill == 0 {
            return Landing::Play;
        }
        if fill < need && state == PENDING {
            self.state.store(WAITED, Ordering::Relaxed);
            return Landing::Wait { in_a_gap: true };
        }
        self.state.store(SETTLED, Ordering::Relaxed);
        Landing::Land {
            excess: fill.saturating_sub(need).min(owed),
            heard: true,
        }
    }

    /// Before the route was ever landed: play the prime until the producer's
    /// first hand-off shows, then land on `need`, whatever queued before the
    /// route's output stream came up.
    #[inline]
    fn first_hand_off(
        &self,
        fill: usize,
        frames: usize,
        (need, cap): (usize, usize),
        state: u8,
    ) -> Landing {
        let landed = if self.first_only { OFF } else { SETTLED };
        if state == PENDING {
            let unfed = self.unfed.load(Ordering::Relaxed);
            if fill <= unfed {
                self.unfed
                    .store(fill.saturating_sub(frames), Ordering::Relaxed);
                return Landing::Play;
            }
            if fill < need {
                if self.first_only {
                    // Waiting would rest the route a buffer deeper than a
                    // lockstep start: it plays what it has, and never lands.
                    self.state.store(OFF, Ordering::Relaxed);
                    return Landing::Play;
                }
                self.state.store(WAITED, Ordering::Relaxed);
                return Landing::Wait { in_a_gap: false };
            }
        }
        self.state.store(landed, Ordering::Relaxed);
        // A route with no slack lands on the buffer it pops; a slack-keeping
        // route drops only what sits above its band (#965: a convolver-fed
        // route is born at its rest and is never cut).
        let top = if self.first_only { need } else { cap };
        Landing::Land {
            excess: fill.saturating_sub(top),
            heard: false,
        }
    }
}
