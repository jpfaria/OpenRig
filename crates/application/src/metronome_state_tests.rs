//! The tap-tempo history and the config→settings restore, moved here from the
//! GUI (#127): both belong to whoever owns the state, and that is now the
//! dispatcher — so a footswitch bound to the tap counts off the same beat the
//! on-screen TAP button does.

use super::*;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

// ── tap_bpm ──────────────────────────────────────────────────────────────

#[test]
fn two_taps_give_the_interval_bpm() {
    // Two taps half a second apart is one interval — 120 BPM.
    assert_eq!(tap_bpm(&[ms(500)]), Some(120.0));
}

#[test]
fn a_single_tap_yields_nothing() {
    // One tap produces no interval, so there is no tempo to report yet.
    assert_eq!(tap_bpm(&[]), None);
}

#[test]
fn a_gap_above_two_seconds_restarts_the_count() {
    // The player stopped, thought about it, and counted off again: everything
    // before the long gap is a different count and must not drag the average.
    assert_eq!(
        tap_bpm(&[ms(1000), ms(1000), ms(2500), ms(500), ms(500)]),
        Some(120.0)
    );
    // A long gap as the LAST interval leaves a single fresh tap — nothing yet.
    assert_eq!(tap_bpm(&[ms(500), ms(500), ms(2500)]), None);
}

#[test]
fn only_the_last_four_intervals_count() {
    // Six intervals: averaging all of them gives 90 BPM, the last four give
    // 120 — the window has to follow the player, not their warm-up.
    assert_eq!(
        tap_bpm(&[ms(1000), ms(1000), ms(500), ms(500), ms(500), ms(500)]),
        Some(120.0)
    );
}

#[test]
fn result_is_clamped_to_the_bpm_range() {
    // 100 ms between taps is 600 BPM — past what the generator supports.
    assert_eq!(tap_bpm(&[ms(100)]), Some(BPM_MAX));
    // Two seconds exactly is still one count (not a reset) and lands on the
    // slow end of the range.
    assert_eq!(tap_bpm(&[ms(2000)]), Some(BPM_MIN));
}

// ── the tap history on the dispatcher's state ────────────────────────────

#[test]
fn the_first_tap_of_a_count_off_changes_nothing() {
    let mut state = MetronomeControlState::default();
    assert_eq!(state.tap_at(Instant::now()), None);
}

#[test]
fn four_taps_at_a_steady_tempo_report_it() {
    let mut state = MetronomeControlState::default();
    let start = Instant::now();
    let mut bpm = None;
    for beat in 0..4 {
        bpm = state.tap_at(start + Duration::from_millis(500 * beat));
    }
    assert_eq!(bpm, Some(120.0));
}

// ── persisted config → dispatcher state ──────────────────────────────────

#[test]
fn the_saved_config_comes_back_on_every_setting() {
    let mut state = MetronomeControlState::default();
    state.seed_from_config(&MetronomeConfig {
        bpm: 96.0,
        beats_per_bar: 6,
        subdivision: "sixteenths".into(),
        timbre: "wood".into(),
        volume: 0.4,
        count_in: true,
        output_device: Some("dev:1".into()),
    });

    let settings = state.settings();
    assert_eq!(settings.bpm, 96.0);
    assert_eq!(settings.beats_per_bar, 6);
    assert_eq!(settings.subdivision, Subdivision::Sixteenths);
    assert_eq!(settings.timbre, Timbre::Wood);
    assert_eq!(settings.volume, 0.4);
    assert!(settings.count_in);
    assert_eq!(state.output_key(), Some("dev:1"));
}

#[test]
fn a_config_written_by_hand_cannot_push_the_generator_out_of_range() {
    let mut state = MetronomeControlState::default();
    state.seed_from_config(&MetronomeConfig {
        bpm: 9000.0,
        volume: 4.0,
        subdivision: "quintuplets".into(),
        timbre: "gong".into(),
        ..MetronomeConfig::default()
    });

    let settings = state.settings();
    assert_eq!(settings.bpm, BPM_MAX);
    assert_eq!(settings.volume, 1.0);
    // Unknown enum names fall back to the defaults rather than to nothing.
    assert_eq!(settings.subdivision, Subdivision::Off);
    assert_eq!(settings.timbre, Timbre::Click);
}

#[test]
fn restoring_the_config_never_marks_the_click_as_running() {
    let mut state = MetronomeControlState::default();
    state.seed_from_config(&MetronomeConfig::default());
    assert!(
        !state.running(),
        "the app always boots silent — `MetronomeConfig` has no `enabled` \
         field precisely so no restore path can turn the click on"
    );
}
