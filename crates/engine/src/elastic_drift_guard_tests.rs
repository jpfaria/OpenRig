use super::*;

const PERIOD: usize = 128;

/// Feeds whole windows at a constant fill; returns what the last call asked
/// to discard.
fn windows(guard: &DriftGuard, fill: usize, count: usize, underruns: u64) -> usize {
    let mut skip = 0;
    for _ in 0..count * WINDOW_FRAMES / PERIOD {
        skip = guard.observe(fill, PERIOD, underruns);
    }
    skip
}

#[test]
fn steady_fill_never_asks_to_discard() {
    let guard = DriftGuard::new(300);
    assert_eq!(windows(&guard, 300, 20, 0), 0);
    assert_eq!(guard.trims(), 0);
}

#[test]
fn fill_stuck_above_the_route_own_level_is_discarded_back_to_it() {
    let guard = DriftGuard::new(300);
    windows(&guard, 300, 3, 0);
    assert_eq!(windows(&guard, 556, 1, 0), 256);
    assert_eq!(guard.trims(), 1);
}

#[test]
fn growth_within_the_slack_is_left_alone() {
    let guard = DriftGuard::new(300);
    windows(&guard, 300, 3, 0);
    assert_eq!(windows(&guard, 300 + SLACK_FRAMES, 1, 0), 0);
}

#[test]
fn a_window_that_underran_neither_sets_the_level_nor_trims() {
    let guard = DriftGuard::new(1000);
    // The first window underran at a high fill: it must not become the level.
    windows(&guard, 900, 1, 5);
    windows(&guard, 300, 2, 5);
    assert_eq!(windows(&guard, 556, 1, 5), 256);
}

#[test]
fn the_level_follows_the_lowest_clean_window() {
    let guard = DriftGuard::new(600);
    windows(&guard, 600, 2, 0);
    windows(&guard, 300, 2, 0);
    assert_eq!(windows(&guard, 600, 1, 0), 300);
}

/// #965: the route's input stream ran ahead of its output stream at start-up,
/// so the ring was full when the first window closed. The level is at most
/// the target plus the buffer being popped, never a floor above that.
#[test]
fn a_route_born_above_its_target_is_discarded_back_to_the_target() {
    let guard = DriftGuard::new(64);
    assert_eq!(windows(&guard, 256, 1, 0), 256 - (64 + PERIOD));
    assert_eq!(guard.trims(), 1);
}

/// A lockstep producer leaves one buffer on top of the resting cushion at
/// every callback start; that buffer is not stuck latency.
#[test]
fn one_callback_buffer_above_the_target_is_the_lockstep_rest() {
    let guard = DriftGuard::new(256);
    assert_eq!(windows(&guard, 256 + PERIOD, 3, 0), 0);
    assert_eq!(guard.trims(), 0);
}

#[test]
fn a_route_resting_below_its_target_is_left_where_it_rests() {
    let guard = DriftGuard::new(256);
    assert_eq!(windows(&guard, 128, 3, 0), 0);
    assert_eq!(guard.trims(), 0);
}

/// #965: a route can rest BELOW its designed cushion — a fresh route swapped
/// in by an off-thread live rebuild rests one period lower (measured on the
/// owner's Quantum: 0..64 frames after edits, 64..128 from a cold start).
/// When it underruns once, the gap pushes it one callback higher. The guard
/// kept the low rest it had learned and cut the route straight back — the
/// fragile rest again, the next underrun, the next cut: a pump of clicks and
/// skips. A window that underran forgets the level.
#[test]
fn a_route_that_underran_back_up_to_its_cushion_is_not_cut() {
    let guard = DriftGuard::new(128);
    windows(&guard, 128, 3, 0);
    windows(&guard, 128 + PERIOD, 1, 1);
    assert_eq!(windows(&guard, 128 + PERIOD, 3, 1), 0);
    assert_eq!(guard.trims(), 0);
}
