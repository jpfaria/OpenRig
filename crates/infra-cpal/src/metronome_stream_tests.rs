//! Issue #14 — the metronome callback's buffer contract.
//!
//! `fill_metronome_buffer` is the whole body of the metronome's audio callback,
//! extracted so it can be exercised with no device open. What it must get right
//! is narrow and testable: silence when off, the click routed to the endpoint's channels,
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
        &[],
        &mut generation,
    );

    assert!(
        out.iter().all(|s| *s == 0.0),
        "a disabled metronome must write silence, not leave the buffer dirty"
    );
}

#[test]
fn routes_the_click_only_to_the_endpoint_channels() {
    let shared = MetronomeShared::new(MetronomeSettings::default());
    shared.set_enabled(true);
    let (mut generator, mut scratch, mut generation) = callback_state();
    let channels = 4;
    let mut out = vec![0.0f32; 256 * channels];

    // The project's output endpoint is on channels 2 and 3 of a 4-out device.
    fill_metronome_buffer(
        &mut generator,
        &shared,
        &mut scratch,
        &mut out,
        channels,
        &[2, 3],
        &mut generation,
    );

    let mut on_target = false;
    for frame in out.chunks(channels) {
        assert_eq!(frame[0], 0.0, "channel 0 is not the endpoint — must stay silent");
        assert_eq!(frame[1], 0.0, "channel 1 is not the endpoint — must stay silent");
        assert_eq!(frame[2], frame[3], "the click is the same mono signal on both endpoint channels");
        if frame[2] != 0.0 {
            on_target = true;
        }
    }
    assert!(on_target, "the downbeat must have played on the endpoint channels");
}

#[test]
fn broadcasts_to_all_channels_when_no_target_is_in_range() {
    // A stale/empty endpoint must not go silent — fall back to every channel.
    let shared = MetronomeShared::new(MetronomeSettings::default());
    shared.set_enabled(true);
    let (mut generator, mut scratch, mut generation) = callback_state();
    let channels = 2;
    let mut out = vec![0.0f32; 256 * channels];

    fill_metronome_buffer(
        &mut generator,
        &shared,
        &mut scratch,
        &mut out,
        channels,
        &[9], // out of range for a 2-channel device
        &mut generation,
    );

    assert!(
        out.iter().any(|s| *s != 0.0),
        "an out-of-range endpoint must still make sound, not silence"
    );
    for frame in out.chunks(channels) {
        assert_eq!(frame[0], frame[1], "the fallback writes the same click to every channel");
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
        &[],
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
        &[],
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
        &[],
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
        &[],
        &mut generation,
    );

    assert_eq!(
        shared.position().beat,
        1,
        "after two beats the published position should be beat 2 of the bar"
    );
}
