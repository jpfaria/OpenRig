//! Issue #14 — the control side and the metronome's audio callback meet here,
//! so these tests pin the two things that keep the callback honest: it never
//! reads a torn value, and it never has to re-read every field per buffer.

use super::*;
use feature_dsp::metronome::{Subdivision, Timbre};

#[test]
fn enabled_defaults_to_false() {
    let shared = MetronomeShared::new(MetronomeSettings::default());
    assert!(
        !shared.enabled(),
        "the metronome must never start playing on its own"
    );
}

#[test]
fn settings_round_trip() {
    let shared = MetronomeShared::new(MetronomeSettings::default());
    let wanted = MetronomeSettings {
        bpm: 137.5,
        beats_per_bar: 7,
        subdivision: Subdivision::Triplets,
        timbre: Timbre::Wood,
        volume: 0.42,
        count_in: true,
    };
    shared.set_settings(wanted);

    let got = shared.settings();
    assert!((got.bpm - wanted.bpm).abs() < 0.01, "bpm was {}", got.bpm);
    assert_eq!(got.beats_per_bar, wanted.beats_per_bar);
    assert_eq!(got.subdivision, wanted.subdivision);
    assert_eq!(got.timbre, wanted.timbre);
    assert!(
        (got.volume - wanted.volume).abs() < 0.001,
        "volume was {}",
        got.volume
    );
    assert_eq!(got.count_in, wanted.count_in);
}

#[test]
fn set_settings_bumps_generation() {
    let shared = MetronomeShared::new(MetronomeSettings::default());
    let before = shared.generation();
    shared.set_settings(MetronomeSettings {
        bpm: 90.0,
        ..Default::default()
    });
    assert!(
        shared.generation() > before,
        "the callback learns a change happened by comparing generations"
    );
}

#[test]
fn position_round_trips_through_the_atomic() {
    let shared = MetronomeShared::new(MetronomeSettings::default());
    let pos = BeatPosition {
        bar: 12,
        beat: 3,
        tick: 2,
        counting_in: true,
    };
    shared.publish_position(pos);
    assert_eq!(shared.position(), pos);
}

#[test]
fn take_restart_is_one_shot() {
    let shared = MetronomeShared::new(MetronomeSettings::default());
    assert!(!shared.take_restart(), "nothing requested yet");
    shared.request_restart();
    assert!(shared.take_restart(), "the request is delivered once");
    assert!(!shared.take_restart(), "and never delivered twice");
}

#[test]
fn enabled_round_trips() {
    let shared = MetronomeShared::new(MetronomeSettings::default());
    shared.set_enabled(true);
    assert!(shared.enabled());
    shared.set_enabled(false);
    assert!(!shared.enabled());
}
