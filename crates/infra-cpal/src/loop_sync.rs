//! Responsibility: maps host-clock instants onto the shared loop timeline.
//!
//! Every looper of the project plays on one timeline: an anchor instant (the
//! top of every loop) on the host clock, and loop lengths that are whole
//! multiples of the shortest one. This is the arithmetic of that timeline.

/// How far ahead a cold-armed loop starts, so its render is ready in time.
pub(crate) const COLD_LEAD_NS: u64 = 100_000_000;
/// How far ahead PLAY from silence puts the top of the loops, so every loop's
/// stream is armed before the downbeat.
pub(crate) const PLAY_LEAD_NS: u64 = 300_000_000;

const NS_PER_S: u128 = 1_000_000_000;

/// Frames at `rate` from `from_ns` to `to_ns`, to the nearest frame; 0 when
/// `to_ns` is not later.
pub(crate) fn frames_between(from_ns: u64, to_ns: u64, rate: u32) -> u64 {
    let Some(span) = to_ns.checked_sub(from_ns) else {
        return 0;
    };
    ((span as u128 * rate as u128 + NS_PER_S / 2) / NS_PER_S) as u64
}

/// The loop position (frames) instant `at_ns` falls on, for a loop of
/// `loop_len` frames whose top is `anchor_ns`.
pub(crate) fn phase_frames(anchor_ns: u64, at_ns: u64, rate: u32, loop_len: usize) -> usize {
    let len = loop_len.max(1) as u64;
    let phase = if at_ns >= anchor_ns {
        frames_between(anchor_ns, at_ns, rate) % len
    } else {
        (len - frames_between(at_ns, anchor_ns, rate) % len) % len
    };
    phase as usize
}

/// The length a take of `captured` frames gets: the nearest whole number of
/// `cycle`-frame cycles, never less than one. A zero cycle keeps it as is.
pub(crate) fn quantize_take_len(captured: usize, cycle: usize) -> usize {
    if cycle == 0 {
        return captured;
    }
    ((captured + cycle / 2) / cycle).max(1) * cycle
}

/// Where and when a loop armed cold at `now_ns` starts: never before one lead
/// from now nor before the anchor, at the loop position the timeline is at
/// then. Returns `(start position, start instant)`.
pub(crate) fn cold_start(anchor_ns: u64, now_ns: u64, rate: u32, loop_len: usize) -> (usize, u64) {
    let at = (now_ns + COLD_LEAD_NS).max(anchor_ns);
    (phase_frames(anchor_ns, at, rate, loop_len), at)
}

#[cfg(test)]
#[path = "loop_sync_tests.rs"]
mod tests;
