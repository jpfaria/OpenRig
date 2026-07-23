//! Issue #14 — the metronome callback's buffer contract.
//!
//! `fill_metronome_buffer` is the whole body of the metronome's audio callback,
//! extracted so it can be exercised with no device open. What it must get right
//! is narrow and testable: silence when off, the same click on every channel,
//! and picking up control-side changes without re-reading settings per buffer.

use super::*;
use engine::metronome_state::{MetronomeSettings, MetronomeShared};

/// A generator and the scratch buffer the callback owns.
fn callback_state() -> (MetronomeGenerator, Vec<f32>, u64) {
    let settings = MetronomeSettings::default();
    (
        MetronomeGenerator::new(48_000.0, settings),
        vec![0.0; 512],
        0,
    )
}

#[test]
fn silent_when_disabled() {
    let shared = MetronomeShared::new(MetronomeSettings::default());
    let (mut generator, mut scratch, mut generation) = callback_state();
    let mut out = vec![1.0f32; 512 * 2];

    fill_metronome_buffer(
        &mut generator,
        &shared,
        &mut scratch,
        &mut out,
        2,
        &mut generation,
    );

    assert!(
        out.iter().all(|s| *s == 0.0),
        "a disabled metronome must write silence, not leave the buffer dirty"
    );
}

#[test]
fn writes_the_same_click_to_every_channel() {
    let shared = MetronomeShared::new(MetronomeSettings::default());
    shared.set_enabled(true);
    let (mut generator, mut scratch, mut generation) = callback_state();
    let channels = 4;
    let mut out = vec![0.0f32; 256 * channels];

    fill_metronome_buffer(
        &mut generator,
        &shared,
        &mut scratch,
        &mut out,
        channels,
        &mut generation,
    );

    assert!(
        out.iter().any(|s| *s != 0.0),
        "an enabled metronome should have rendered its downbeat"
    );
    for frame in out.chunks(channels) {
        for s in frame {
            assert_eq!(
                *s, frame[0],
                "every channel carries the same mono click, undivided"
            );
        }
    }
}

#[test]
fn picks_up_a_settings_change_via_generation() {
    let shared = MetronomeShared::new(MetronomeSettings::default());
    shared.set_enabled(true);
    let (mut generator, mut scratch, mut generation) = callback_state();
    let mut out = vec![0.0f32; 256 * 2];

    fill_metronome_buffer(
        &mut generator,
        &shared,
        &mut scratch,
        &mut out,
        2,
        &mut generation,
    );
    let after_first = generation;

    shared.set_settings(MetronomeSettings {
        bpm: 200.0,
        ..Default::default()
    });
    fill_metronome_buffer(
        &mut generator,
        &shared,
        &mut scratch,
        &mut out,
        2,
        &mut generation,
    );

    assert!(
        generation > after_first,
        "the callback should have caught up to the new generation"
    );
    assert!(
        (generator.settings().bpm - 200.0).abs() < 0.01,
        "the change should have reached the generator"
    );
}

#[test]
fn a_restart_request_is_consumed() {
    let shared = MetronomeShared::new(MetronomeSettings::default());
    shared.set_enabled(true);
    shared.request_restart();
    let (mut generator, mut scratch, mut generation) = callback_state();
    let mut out = vec![0.0f32; 256 * 2];

    fill_metronome_buffer(
        &mut generator,
        &shared,
        &mut scratch,
        &mut out,
        2,
        &mut generation,
    );

    assert!(
        !shared.take_restart(),
        "the callback must consume the restart, or the bar restarts forever"
    );
}

#[test]
fn the_position_reaches_the_ui() {
    let shared = MetronomeShared::new(MetronomeSettings {
        bpm: 240.0,
        beats_per_bar: 4,
        ..Default::default()
    });
    shared.set_enabled(true);
    let (mut generator, mut scratch, mut generation) = callback_state();
    // Two beats at 240 bpm and 48 kHz.
    let mut out = vec![0.0f32; 24_000 * 2];

    fill_metronome_buffer(
        &mut generator,
        &shared,
        &mut scratch,
        &mut out,
        2,
        &mut generation,
    );

    assert_eq!(
        shared.position().beat,
        1,
        "after two beats the published position should be beat 2 of the bar"
    );
}
