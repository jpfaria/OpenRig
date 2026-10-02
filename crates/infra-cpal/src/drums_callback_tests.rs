//! The drums callback's buffer contract, exercised with no device open.

use std::sync::Arc;

use engine::drum_state::{
    DrumHit, DrumKit, DrumLayer, DrumPattern, DrumPiece, DrumRole, DrumsShared, Groove,
};

use super::DrumsCallback;

const RATE: u32 = 48_000;

/// A kit whose kick is a constant 1.0 for a while, panned by `pan`.
fn kit(rate: u32, pan: f32) -> DrumKit {
    let mut kit = DrumKit::new("test", rate);
    kit.set_piece(
        DrumRole::Kick,
        DrumPiece {
            layers: vec![DrumLayer {
                min_velocity: 0.0,
                max_velocity: 1.0,
                samples: vec![Arc::from(vec![1.0f32; 4096])],
                gain: 1.0,
            }],
            gain: 1.0,
            pan,
            choke_group: None,
        },
    );
    kit
}

/// A kick on every beat.
fn groove() -> Groove {
    let hits = (0..4)
        .map(|beat| DrumHit {
            beat: f64::from(beat),
            role: DrumRole::Kick,
            velocity: 1.0,
        })
        .collect();
    Groove {
        id: "g".into(),
        name: "g".into(),
        genre: "test".into(),
        beats_per_bar: 4,
        tempo: 120.0,
        beat: DrumPattern::new(4.0, hits),
        fills: vec![DrumPattern::new(4.0, Vec::new())],
    }
}

fn playing_shared(pan: f32) -> DrumsShared {
    let shared = DrumsShared::default();
    shared.kits().send(Arc::new(kit(RATE, pan)));
    shared.grooves().send(Arc::new(groove()));
    shared.set_enabled(true);
    shared.set_playing(true);
    shared
}

#[test]
fn silent_when_disabled() {
    let shared = DrumsShared::default();
    let mut callback = DrumsCallback::new(&shared, RATE, 256);
    let mut out = vec![1.0f32; 256 * 2];

    callback.fill(&shared, &mut out, 2, &[0, 1]);

    assert!(out.iter().all(|s| *s == 0.0));
}

#[test]
fn plays_left_and_right_on_the_endpoint_channels_only() {
    let shared = playing_shared(-1.0);
    let mut callback = DrumsCallback::new(&shared, RATE, 256);
    let channels = 4;
    let mut out = vec![0.0f32; 256 * channels];

    callback.fill(&shared, &mut out, channels, &[2, 3]);

    let left: f32 = out.chunks(channels).map(|f| f[2].abs()).sum();
    let right: f32 = out.chunks(channels).map(|f| f[3].abs()).sum();
    let others: f32 = out.chunks(channels).map(|f| f[0].abs() + f[1].abs()).sum();
    assert!(left > 0.0, "a hard-left kick lands on the endpoint's left");
    assert!(right < 1e-6, "and not on its right");
    assert_eq!(others, 0.0, "nothing reaches channels outside the endpoint");
}

#[test]
fn a_mono_endpoint_gets_the_average_of_left_and_right() {
    let shared = playing_shared(-1.0);
    let mut stereo = DrumsCallback::new(&shared, RATE, 64);
    let mut reference = vec![0.0f32; 64 * 2];
    stereo.fill(&shared, &mut reference, 2, &[0, 1]);

    let shared = playing_shared(-1.0);
    let mut mono = DrumsCallback::new(&shared, RATE, 64);
    let mut out = vec![0.0f32; 64 * 2];
    mono.fill(&shared, &mut out, 2, &[1]);

    for (frame, stereo_frame) in out.chunks(2).zip(reference.chunks(2)) {
        let expected = (stereo_frame[0] + stereo_frame[1]) * 0.5;
        assert!((frame[1] - expected).abs() < 1e-6);
        assert_eq!(frame[0], 0.0);
    }
}

#[test]
fn picks_up_a_kit_sent_after_the_stream_opened() {
    let shared = DrumsShared::default();
    shared.grooves().send(Arc::new(groove()));
    shared.set_enabled(true);
    shared.set_playing(true);
    let mut callback = DrumsCallback::new(&shared, RATE, 128);
    let mut out = vec![0.0f32; 128 * 2];
    callback.fill(&shared, &mut out, 2, &[0, 1]);
    assert!(out.iter().all(|s| *s == 0.0), "no kit yet, nothing to hear");

    shared.kits().send(Arc::new(kit(RATE, 0.0)));
    shared.set_playing(false);
    callback.fill(&shared, &mut out, 2, &[0, 1]);
    shared.set_playing(true);
    callback.fill(&shared, &mut out, 2, &[0, 1]);

    assert!(out.iter().any(|s| *s != 0.0), "the new kit plays");
}

#[test]
fn ignores_a_kit_prepared_for_another_sample_rate() {
    let shared = DrumsShared::default();
    shared.kits().send(Arc::new(kit(44_100, 0.0)));
    shared.grooves().send(Arc::new(groove()));
    shared.set_enabled(true);
    shared.set_playing(true);
    let mut callback = DrumsCallback::new(&shared, RATE, 128);
    let mut out = vec![0.0f32; 128 * 2];

    callback.fill(&shared, &mut out, 2, &[0, 1]);

    assert!(
        out.iter().all(|s| *s == 0.0),
        "a kit at the wrong rate would play out of tune"
    );
}

#[test]
fn publishes_the_position_and_follows_the_transport() {
    let shared = playing_shared(0.0);
    let mut callback = DrumsCallback::new(&shared, RATE, 128);
    let mut out = vec![0.0f32; 128 * 2];

    callback.fill(&shared, &mut out, 2, &[0, 1]);
    assert!(shared.position().playing);

    shared.set_playing(false);
    callback.fill(&shared, &mut out, 2, &[0, 1]);
    assert!(!shared.position().playing);
}

#[test]
fn a_fill_request_starts_a_fill() {
    let shared = playing_shared(0.0);
    let mut callback = DrumsCallback::new(&shared, RATE, 128);
    let mut out = vec![0.0f32; 128 * 2];
    callback.fill(&shared, &mut out, 2, &[0, 1]);

    shared.request_fill();
    callback.fill(&shared, &mut out, 2, &[0, 1]);

    assert!(shared.position().in_fill);
}

#[test]
fn a_tempo_change_reaches_the_machine() {
    let shared = playing_shared(0.0);
    let mut callback = DrumsCallback::new(&shared, RATE, 512);
    let mut out = vec![0.0f32; 512 * 2];
    shared.set_settings(engine::drum_state::DrumSettings {
        bpm: 400.0,
        volume: 1.0,
    });
    // At 400 bpm a beat is 7200 frames: 15 buffers of 512 cross beat 1.
    for _ in 0..15 {
        callback.fill(&shared, &mut out, 2, &[0, 1]);
    }
    assert_eq!(shared.position().beat, 1);
}
