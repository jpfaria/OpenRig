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
    }
}

#[cfg(test)]
#[path = "route_cushion_tests.rs"]
mod tests;
