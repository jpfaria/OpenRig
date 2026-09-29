//! Responsibility: sizes one output route's elastic cushion.
//!
//! A route's cushion is the latency it rests at between the producer (the
//! chain DSP) and its output callback. #965 made it one rule for every route:
//!
//! - **Never born above its rest.** A route fed by a convolver (#592) is
//!   primed with exactly its own resting cushion — on a cold start, on an
//!   off-thread live rebuild and on a DI render alike. It used to get a
//!   512-frame prime that the drift guard then cut ~186 ms later: a skip of
//!   live audio after every edit and every DI render. A route that starts
//!   at its rest never needs a cut.
//! - **The IR cushion only where the clocks differ.** On its producer's own
//!   device clock a convolver-fed route rests at its lean target: measured
//!   on the owner's Quantum HD 8 at 64 frames, 0 underruns from a cold
//!   start, over a minute, after live edits and after adding a cab live
//!   under full CPU load. On another device the two clocks drift even at
//!   the same nominal rate and the ring slowly drains; there the route keeps
//!   the #592 cushion ([`IR_COLD_START_CUSHION_FRAMES`]), which is what held
//!   the #670 real-streams battery clean.
//! - **One buffer of slack (#979).** A convolver-fed route on the chain's own
//!   clock keeps one device buffer queued behind the buffer each callback
//!   pops. On macOS the #670 worker hands a cycle's buffer over after that
//!   cycle's output callbacks, so a route born with exactly its target (one
//!   buffer on the same device) popped it whole and rested with nothing
//!   queued: a worker one period late was a gap. Its hand-off now lands it on
//!   that slack at the start and after every gap (`elastic_hand_off`) — the
//!   route rests one device buffer deeper, never more, and never above the
//!   target plus one buffer the drift guard already allows; it never holds
//!   more than one buffer past the ring it had without slack.
//! - **One buffer of slack per signal path.** The owner allowed ONE buffer.
//!   A guitar through an insert loop crosses two rings — the send, then the
//!   tail after the return — so an insert send never keeps slack: an IR
//!   before the insert would otherwise give the loop two. The slack sits on
//!   the tail; one late worker pass still costs the send the buffer it did
//!   not deliver.
//! - **Never deeper for a late stream (#979).** A route on its producer's
//!   clock lands on its rest at the producer's first hand-off, dropping what
//!   queued before its output stream came up — before anything was heard. A
//!   route born empty (no convolver) used to keep one buffer per input
//!   period that ran before its stream did, up to its whole ring, for the
//!   life of the chain: two outputs of one guitar a buffer apart.
//! - **Another clock, another owner.** A route on another device clock (#85)
//!   rests [`CROSS_RATE_CUSHION`] times deeper and its level is held by the
//!   resampler's servo, so the #953 drift guard must not also trim it: the
//!   two fought, refill and cut, for the life of the route.

/// #85: how much deeper a cross-rate route's cushion is. Three device buffers
/// covers the ordinary case of the OS handing a device two periods at once
/// while the producer is still on its own clock; the extra latency lands ONLY
/// on that tap, never on the chain's own output.
const CROSS_RATE_CUSHION: usize = 3;

/// #592: the cushion a convolver-fed route keeps when it runs on another
/// clock than its producer.
pub(crate) const IR_COLD_START_CUSHION_FRAMES: usize = 512;

/// How one output route is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RouteCushion {
    /// Frames the route rests at between producer and consumer.
    pub(crate) target: usize,
    /// Frames the ring can hold.
    pub(crate) capacity: usize,
    /// Silence a fresh route starts with, never more than `target`.
    pub(crate) prime: usize,
    /// The #85 resampler servo owns the level; the drift guard stays off.
    pub(crate) servo_owned: bool,
    /// #979: the route keeps one device buffer of slack on top of the buffer
    /// each callback pops (see `elastic_hand_off`).
    pub(crate) keeps_slack: bool,
    /// #979: on its producer's own clock, the route lands on its rest at the
    /// producer's first hand-off, whatever queued before its output stream
    /// came up (a slack-keeping route also after every gap).
    pub(crate) lands_on_first_hand_off: bool,
}

/// The cushion of a route whose lockstep (same-clock) target is
/// `lockstep_target`, running at `route_rate` for a chain at `runtime_rate`,
/// with `fed_by_convolver` when an IR/cab convolver feeds it and
/// `on_producer_clock` when its output device is its producer's input
/// device. Shared by the initial build and the live rebuild, so a rebuilt
/// route is sized exactly like the one it replaces.
pub(crate) fn route_cushion(
    lockstep_target: usize,
    route_rate: f32,
    runtime_rate: f32,
    fed_by_convolver: bool,
    on_producer_clock: bool,
) -> RouteCushion {
    let lockstep_target = if fed_by_convolver && !on_producer_clock {
        lockstep_target.max(IR_COLD_START_CUSHION_FRAMES)
    } else {
        lockstep_target
    };
    let cross_rate = (route_rate - runtime_rate).abs() >= f32::EPSILON;
    let target = if cross_rate {
        lockstep_target.saturating_mul(CROSS_RATE_CUSHION)
    } else {
        lockstep_target
    };
    // A cross-rate route's extra depth only helps if it is actually filled
    // (#85); a convolver-fed route starts with its whole cushion (#592).
    let prime = if fed_by_convolver {
        target
    } else {
        target - lockstep_target.min(target)
    };
    RouteCushion {
        target,
        capacity: target.saturating_mul(2),
        prime,
        servo_owned: cross_rate,
        // #979: a route born with a cushion on the chain's own clock keeps
        // one buffer of slack behind the one it pops, so a worker one period
        // late costs no gap. A route no convolver feeds is born empty and
        // rests at the hand-off, as before.
        keeps_slack: fed_by_convolver && !cross_rate,
        // #979: a route's latency never depends on when its output stream
        // came up. On its producer's clock the hand-off is lockstep (#965),
        // so the fill at its first callback past the prime says how many
        // input periods ran before the stream did; on another clock (or with
        // the #85 servo holding its level) it says nothing.
        // Only a route born EMPTY lands there: one a convolver primed is born
        // at its rest (#965) and is never cut below it (#980).
        lands_on_first_hand_off: !fed_by_convolver && on_producer_clock && !cross_rate,
    }
}

impl RouteCushion {
    /// #979: this route is an insert SEND — its signal comes back through the
    /// return into a tail route that keeps the path's one buffer of slack.
    pub(crate) fn for_an_insert_send(self) -> Self {
        Self {
            keeps_slack: false,
            ..self
        }
    }
}

#[cfg(test)]
#[path = "route_cushion_tests.rs"]
mod tests;
