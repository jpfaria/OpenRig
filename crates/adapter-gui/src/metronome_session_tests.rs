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

// ── the tap history on the session ───────────────────────────────────────

fn session() -> MetronomeSession {
    MetronomeSession::from_config(&MetronomeConfig::default())
}

#[test]
fn the_first_tap_of_a_count_off_changes_nothing() {
    let mut s = session();
    assert_eq!(s.tap_at(Instant::now()), None);
}

#[test]
fn four_taps_at_a_steady_tempo_report_it() {
    let mut s = session();
    let start = Instant::now();
    let mut bpm = None;
    for beat in 0..4 {
        bpm = s.tap_at(start + Duration::from_millis(500 * beat));
    }
    assert_eq!(bpm, Some(120.0));
}

// ── knob index ↔ command key ─────────────────────────────────────────────

#[test]
fn the_time_signature_knob_walks_the_seven_supported_bars() {
    let beats: Vec<u32> = (0..7).map(time_signature_beats).collect();
    assert_eq!(beats, vec![2, 3, 4, 5, 6, 7, 12]);
}

#[test]
fn the_time_signature_knob_rests_on_four_four() {
    let s = session();
    assert_eq!(s.time_signature_index(), 2);
    assert_eq!(s.time_signature_label(), "4/4");
    assert_eq!(s.beats_per_bar(), 4);
}

#[test]
fn a_bar_length_the_knob_cannot_express_falls_back_to_four_four() {
    // An MCP client may ask for 9 beats; the knob has no position for it, so it
    // must not point at a random one.
    assert_eq!(time_signature_index(9), 2);
}

#[test]
fn the_subdivision_knob_speaks_the_commands_dispatcher_vocabulary() {
    let keys: Vec<&str> = (0..4).map(subdivision_key).collect();
    assert_eq!(keys, vec!["off", "eighths", "triplets", "sixteenths"]);
}

#[test]
fn the_timbre_knob_speaks_the_commands_dispatcher_vocabulary() {
    let keys: Vec<&str> = (0..3).map(timbre_key).collect();
    assert_eq!(keys, vec!["click", "wood", "beep"]);
}

#[test]
fn a_knob_index_past_the_last_position_stays_on_the_last_one() {
    // The knob cycles in Slint, but nothing may produce an out-of-range key.
    assert_eq!(subdivision_key(99), "sixteenths");
    assert_eq!(timbre_key(-1), "click");
    assert_eq!(time_signature_beats(99), 12);
}

#[test]
fn an_event_key_comes_back_as_the_knob_position_that_produced_it() {
    let mut s = session();
    s.set_subdivision_key("triplets");
    assert_eq!(s.subdivision_index(), 2);
    assert_eq!(s.subdivision_label(), "1/8T");
    s.set_timbre_key("beep");
    assert_eq!(s.timbre_index(), 2);
}

// ── persisted config → window state ──────────────────────────────────────

#[test]
fn the_saved_config_comes_back_on_every_control() {
    let config = MetronomeConfig {
        bpm: 96.0,
        beats_per_bar: 6,
        subdivision: "sixteenths".into(),
        timbre: "wood".into(),
        volume: 0.4,
        count_in: true,
        output_device: Some("dev:1".into()),
    };
    let s = MetronomeSession::from_config(&config);
    assert_eq!(s.bpm(), 96.0);
    assert_eq!(s.time_signature_index(), 4);
    assert_eq!(s.time_signature_label(), "6/8");
    assert_eq!(s.subdivision_index(), 3);
    assert_eq!(s.timbre_index(), 1);
    assert_eq!(s.volume(), 0.4);
    assert!(s.count_in());
    assert_eq!(s.output_device(), Some("dev:1"));
}

#[test]
fn a_config_written_by_hand_cannot_push_the_generator_out_of_range() {
    let config = MetronomeConfig {
        bpm: 9000.0,
        volume: 4.0,
        subdivision: "quintuplets".into(),
        timbre: "gong".into(),
        ..MetronomeConfig::default()
    };
    let s = MetronomeSession::from_config(&config);
    assert_eq!(s.bpm(), BPM_MAX);
    assert_eq!(s.volume(), 1.0);
    // Unknown enum names fall back to the defaults rather than to nothing.
    assert_eq!(s.subdivision_key(), "off");
    assert_eq!(s.timbre_key(), "click");
}

// ── output device resolution ─────────────────────────────────────────────

#[test]
fn the_saved_output_device_is_used_while_it_is_connected() {
    let devices = vec!["dev:a".to_string(), "dev:b".to_string()];
    assert_eq!(
        resolve_output_device(Some("dev:b"), &devices),
        Some("dev:b".to_string())
    );
}

#[test]
fn an_unplugged_output_device_falls_back_to_the_first_available() {
    let devices = vec!["dev:a".to_string()];
    assert_eq!(
        resolve_output_device(Some("dev:gone"), &devices),
        Some("dev:a".to_string())
    );
    assert_eq!(
        resolve_output_device(None, &devices),
        Some("dev:a".to_string())
    );
}

#[test]
fn a_machine_with_no_output_resolves_to_nothing() {
    assert_eq!(resolve_output_device(Some("dev:a"), &[]), None);
    assert_eq!(resolve_output_device(None, &[]), None);
}
