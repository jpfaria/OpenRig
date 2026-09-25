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

/// #980: a convolver-fed route is born primed at its cushion (#592) — the
/// margin that absorbs a dsp-worker buffer landing late. One lucky clean
/// window can see the ring down at a single buffer; that window must not
/// become the level the route is trimmed back to, or the route loses its
/// whole margin and every late worker buffer (a VST3 enabled live) is an
/// underrun on the callbacks whose phase sits near the worker's finish time.
#[test]
fn issue_980_a_primed_route_is_never_trimmed_below_its_prime() {
    const PRIME: usize = 256;
    let guard = DriftGuard::new(PRIME);
    guard.hold_rest(PRIME);
    // A clean window where the worker happened to land just in time.
    windows(&guard, PERIOD, 1, 0);
    // Back at the primed lockstep rest: the prime + the buffer being popped.
    assert_eq!(windows(&guard, PRIME + PERIOD, 3, 0), 0);
    assert_eq!(guard.trims(), 0);
}
