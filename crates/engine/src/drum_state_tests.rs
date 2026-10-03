//! The drum machine's control side and its audio callback meet here.

use super::*;

fn settings(bpm: f32, volume: f32) -> DrumSettings {
    DrumSettings { bpm, volume }
}

#[test]
fn starts_disabled_and_stopped() {
    let shared = DrumsShared::default();
    assert!(!shared.enabled(), "drums never start on their own");
    assert!(!shared.playing());
    assert_eq!(shared.settings(), DEFAULT_DRUM_SETTINGS);
}

#[test]
fn settings_round_trip_and_bump_the_generation() {
    let shared = DrumsShared::new(settings(120.0, 0.5));
    let before = shared.generation();

    shared.set_settings(settings(97.25, 0.8));

    assert_eq!(shared.settings(), settings(97.25, 0.8));
    assert!(shared.generation() > before);
}

#[test]
fn fill_requests_count_up() {
    let shared = DrumsShared::default();
    let before = shared.fill_requests();
    shared.request_fill();
    shared.request_fill();
    assert_eq!(shared.fill_requests(), before.wrapping_add(2));
}

#[test]
fn transport_flags_round_trip() {
    let shared = DrumsShared::default();
    shared.set_enabled(true);
    shared.set_playing(true);
    assert!(shared.enabled());
    assert!(shared.playing());
}

#[test]
fn position_round_trips() {
    let shared = DrumsShared::default();
    let position = DrumPosition {
        playing: true,
        bar: 41,
        beat: 3,
        in_fill: true,
    };
    shared.publish_position(position);
    assert_eq!(shared.position(), position);
}

#[test]
fn kits_and_grooves_have_their_own_handoffs() {
    let shared = DrumsShared::default();
    shared.kits().send(Arc::new(DrumKit::new("k", 48_000)));
    shared.grooves().send(Arc::new(silent_groove()));
    assert_eq!(
        shared.kits().take_latest().map(|k| k.sample_rate()),
        Some(48_000)
    );
    assert!(shared.grooves().take_latest().is_some());
}

#[test]
fn silent_groove_has_no_hits() {
    let groove = silent_groove();
    assert!(groove.beat.hits().is_empty());
    assert!(groove.fills.is_empty());
    assert!(groove.beats_per_bar >= 1);
}

#[test]
fn a_slow_kit_load_never_replaces_a_newer_choice() {
    let shared = DrumsShared::default();
    let first = shared.begin_kit_load();
    let second = shared.begin_kit_load();

    assert!(shared.finish_kit_load(second, Arc::new(DrumKit::new("second", 48_000))));
    assert!(
        !shared.finish_kit_load(first, Arc::new(DrumKit::new("first", 48_000))),
        "the older request finished last and must be dropped"
    );
    assert_eq!(
        shared.kits().latest().map(|k| k.name().to_string()),
        Some("second".to_string())
    );
}
