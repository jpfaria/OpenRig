//! The shared loop timeline's arithmetic: host-clock nanoseconds to frames,
//! the phase of an instant in a loop, and the length a take is rounded to.

use super::*;

const MS: u64 = 1_000_000;
const S: u64 = 1_000 * MS;

#[test]
fn frames_between_counts_frames_at_the_given_rate() {
    assert_eq!(frames_between(0, S, 48_000), 48_000);
    assert_eq!(frames_between(5 * S, 5 * S + 10 * MS, 1_000), 10);
    assert_eq!(
        frames_between(0, 20_834, 48_000),
        1,
        "rounds to the nearest frame, not down"
    );
}

#[test]
fn frames_between_an_instant_and_an_earlier_one_is_zero() {
    assert_eq!(frames_between(2 * S, S, 48_000), 0);
}

#[test]
fn the_phase_of_an_instant_wraps_at_the_loop_length() {
    // 1 kHz: one frame per millisecond. A 2000-frame loop anchored at 1 s.
    assert_eq!(phase_frames(S, S, 1_000, 2_000), 0);
    assert_eq!(phase_frames(S, S + 500 * MS, 1_000, 2_000), 500);
    assert_eq!(
        phase_frames(S, 3 * S, 1_000, 2_000),
        0,
        "a whole loop later"
    );
    assert_eq!(phase_frames(S, 3 * S + 250 * MS, 1_000, 2_000), 250);
}

#[test]
fn the_phase_of_an_instant_before_the_anchor_counts_back_from_the_end() {
    // 250 ms before the anchor is 250 frames before the top of the loop.
    assert_eq!(phase_frames(S, S - 250 * MS, 1_000, 2_000), 1_750);
    assert_eq!(phase_frames(3 * S, S, 1_000, 2_000), 0);
}

#[test]
fn a_take_rounds_to_the_nearest_whole_number_of_cycles() {
    assert_eq!(quantize_take_len(3_700, 2_000), 4_000, "1.85 cycles → 2");
    assert_eq!(quantize_take_len(4_900, 2_000), 4_000, "2.45 cycles → 2");
    assert_eq!(quantize_take_len(5_100, 2_000), 6_000, "2.55 cycles → 3");
    assert_eq!(quantize_take_len(2_000, 2_000), 2_000);
}

#[test]
fn a_take_shorter_than_half_a_cycle_still_lasts_one_cycle() {
    assert_eq!(quantize_take_len(600, 2_000), 2_000);
    assert_eq!(quantize_take_len(0, 2_000), 2_000);
}

#[test]
fn a_cold_start_before_the_anchor_waits_for_the_anchor_at_the_top() {
    // The anchor is still 300 ms away: the loop starts exactly there, at 0.
    let (pos, at) = cold_start(10 * S, 10 * S - 300 * MS, 1_000, 2_000);
    assert_eq!(at, 10 * S);
    assert_eq!(pos, 0);
}

#[test]
fn a_cold_start_after_the_anchor_joins_mid_loop_in_phase() {
    // Anchored at 1 s; armed at 4.2 s. It starts one lead later, at the loop
    // position every other loop on the timeline is at by then.
    let now = 4 * S + 200 * MS;
    let (pos, at) = cold_start(S, now, 1_000, 2_000);
    assert_eq!(at, now + COLD_LEAD_NS);
    assert_eq!(pos, phase_frames(S, at, 1_000, 2_000));
    assert_eq!(pos, (3_200 + (COLD_LEAD_NS / MS) as usize) % 2_000);
}

#[test]
fn a_cold_start_with_the_anchor_inside_the_lead_starts_one_lead_ahead() {
    // The anchor is 10 ms away but the render needs a lead: start after it,
    // already in phase, rather than racing the anchor.
    let (pos, at) = cold_start(10 * S + 10 * MS, 10 * S, 1_000, 2_000);
    assert_eq!(at, 10 * S + COLD_LEAD_NS);
    assert_eq!(pos, ((COLD_LEAD_NS / MS) as usize) - 10);
}
