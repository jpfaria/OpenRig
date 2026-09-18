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
    let guard = DriftGuard::new();
    assert_eq!(windows(&guard, 300, 20, 0), 0);
    assert_eq!(guard.trims(), 0);
}

#[test]
fn fill_stuck_above_the_route_own_level_is_discarded_back_to_it() {
    let guard = DriftGuard::new();
    windows(&guard, 300, 3, 0);
    assert_eq!(windows(&guard, 556, 1, 0), 256);
    assert_eq!(guard.trims(), 1);
}

#[test]
fn growth_within_the_slack_is_left_alone() {
    let guard = DriftGuard::new();
    windows(&guard, 300, 3, 0);
    assert_eq!(windows(&guard, 300 + SLACK_FRAMES, 1, 0), 0);
}

#[test]
fn a_window_that_underran_neither_sets_the_level_nor_trims() {
    let guard = DriftGuard::new();
    // The first window underran at a high fill: it must not become the level.
    windows(&guard, 900, 1, 5);
    windows(&guard, 300, 2, 5);
    assert_eq!(windows(&guard, 556, 1, 5), 256);
}

#[test]
fn the_level_follows_the_lowest_clean_window() {
    let guard = DriftGuard::new();
    windows(&guard, 600, 2, 0);
    windows(&guard, 300, 2, 0);
    assert_eq!(windows(&guard, 600, 1, 0), 300);
}
