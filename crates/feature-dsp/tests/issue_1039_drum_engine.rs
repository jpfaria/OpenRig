//! Issue #1039 — the built-in drum machine's engine contract.
//!
//! Everything renders offline and asserts on sample positions and amplitudes,
//! so no device and no ears are involved. The samples used here are synthetic
//! (impulses and constant runs) so an onset and its level can be read straight
//! off the buffer.

use std::sync::Arc;

use feature_dsp::drums::{
    DrumHit, DrumKit, DrumLayer, DrumMachine, DrumPattern, DrumPiece, DrumRole, DrumSettings,
    Groove,
};

const CENTER: f32 = std::f32::consts::FRAC_1_SQRT_2;

fn samples(value: f32, len: usize) -> Arc<[f32]> {
    vec![value; len].into()
}

fn impulse(value: f32) -> Arc<[f32]> {
    samples(value, 1)
}

fn piece(sample: Arc<[f32]>) -> DrumPiece {
    DrumPiece {
        layers: vec![DrumLayer {
            min_velocity: 0.0,
            max_velocity: 1.0,
            samples: vec![sample],
            gain: 1.0,
        }],
        gain: 1.0,
        pan: 0.0,
        choke_group: None,
    }
}

fn hit(beat: f64, role: DrumRole, velocity: f32) -> DrumHit {
    DrumHit {
        beat,
        role,
        velocity,
    }
}

fn groove(beat: DrumPattern, fills: Vec<DrumPattern>) -> Arc<Groove> {
    Arc::new(Groove {
        id: "test".into(),
        name: "Test".into(),
        genre: "rock".into(),
        beats_per_bar: 4,
        tempo: 120.0,
        beat,
        fills,
    })
}

fn four_on_the_floor(role: DrumRole) -> DrumPattern {
    DrumPattern::new(4.0, (0..4).map(|b| hit(b as f64, role, 1.0)).collect())
}

fn kit_with(pieces: Vec<(DrumRole, DrumPiece)>, rate: u32) -> Arc<DrumKit> {
    let mut kit = DrumKit::new("test", rate);
    for (role, p) in pieces {
        kit.set_piece(role, p);
    }
    Arc::new(kit)
}

fn settings(bpm: f32) -> DrumSettings {
    DrumSettings { bpm, volume: 1.0 }
}

fn render(machine: &mut DrumMachine, frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut l = vec![0.0; frames];
    let mut r = vec![0.0; frames];
    machine.render(&mut l, &mut r);
    (l, r)
}

fn onsets(buf: &[f32]) -> Vec<usize> {
    buf.iter()
        .enumerate()
        .filter(|(_, s)| **s != 0.0)
        .map(|(i, _)| i)
        .collect()
}

fn kick_machine(rate: f32, bpm: f32) -> DrumMachine {
    let kit = kit_with(vec![(DrumRole::Kick, piece(impulse(1.0)))], rate as u32);
    DrumMachine::new(
        rate,
        kit,
        groove(four_on_the_floor(DrumRole::Kick), vec![]),
        settings(bpm),
    )
}

// ── roles ──────────────────────────────────────────────────────────────────

#[test]
fn role_keys_round_trip() {
    for role in DrumRole::ALL {
        assert_eq!(DrumRole::from_key(role.key()), Some(role));
    }
    assert_eq!(DrumRole::from_key("nope"), None);
}

#[test]
fn role_general_midi_mapping_covers_the_kit_pieces() {
    let cases = [
        (35, DrumRole::Kick),
        (36, DrumRole::Kick),
        (37, DrumRole::SideStick),
        (38, DrumRole::Snare),
        (40, DrumRole::SnareRim),
        (41, DrumRole::TomFloor),
        (42, DrumRole::HatClosed),
        (44, DrumRole::HatPedal),
        (45, DrumRole::TomMid),
        (46, DrumRole::HatOpen),
        (48, DrumRole::TomHigh),
        (49, DrumRole::Crash),
        (51, DrumRole::Ride),
        (52, DrumRole::China),
        (53, DrumRole::RideBell),
        (55, DrumRole::Splash),
        (57, DrumRole::Crash2),
    ];
    for (note, role) in cases {
        assert_eq!(DrumRole::from_general_midi(note), Some(role), "note {note}");
    }
    assert_eq!(DrumRole::from_general_midi(0), None);
}

#[test]
fn role_fallback_chain_always_terminates() {
    for role in DrumRole::ALL {
        let mut current = Some(role);
        let mut steps = 0;
        while let Some(r) = current {
            current = r.fallback();
            steps += 1;
            assert!(
                steps <= DrumRole::ALL.len(),
                "{role:?} falls back in a cycle"
            );
        }
    }
}

// ── kit ────────────────────────────────────────────────────────────────────

#[test]
fn kit_resolves_a_missing_piece_through_its_fallback() {
    let kit = kit_with(vec![(DrumRole::Snare, piece(impulse(1.0)))], 48_000);
    assert!(kit.piece_for(DrumRole::Snare).is_some());
    assert!(kit.piece_for(DrumRole::SnareRim).is_some());
    assert!(kit.piece_for(DrumRole::SideStick).is_some());
    assert!(kit.piece_for(DrumRole::Kick).is_none());
}

// ── pattern ────────────────────────────────────────────────────────────────

#[test]
fn pattern_sorts_hits_and_wraps_them_into_its_length() {
    let p = DrumPattern::new(
        4.0,
        vec![
            hit(2.0, DrumRole::Kick, 1.0),
            hit(-0.01, DrumRole::Kick, 1.0),
            hit(1.0, DrumRole::Kick, 1.0),
            hit(4.0, DrumRole::Kick, 1.0),
        ],
    );
    let beats: Vec<f64> = p.hits().iter().map(|h| h.beat).collect();
    assert_eq!(beats.len(), 4);
    assert!((beats[0] - 0.0).abs() < 1e-9);
    assert!((beats[1] - 1.0).abs() < 1e-9);
    assert!((beats[2] - 2.0).abs() < 1e-9);
    assert!((beats[3] - 3.99).abs() < 1e-9);
    assert_eq!(p.beats(), 4.0);
}

// ── timing ─────────────────────────────────────────────────────────────────

#[test]
fn hits_land_on_the_exact_sample_at_any_rate() {
    for &rate in &[44_100.0f32, 48_000.0] {
        let mut m = kick_machine(rate, 120.0);
        m.play();
        let (l, _) = render(&mut m, (rate * 2.0) as usize);
        let half = (rate * 0.5) as usize;
        assert_eq!(onsets(&l), vec![0, half, 2 * half, 3 * half], "rate {rate}");
    }
}

#[test]
fn chunked_render_matches_a_single_render() {
    let mut whole = kick_machine(48_000.0, 133.0);
    whole.play();
    let (expected, _) = render(&mut whole, 96_000);

    let mut chunked = kick_machine(48_000.0, 133.0);
    chunked.play();
    let mut got = Vec::new();
    for _ in 0..(96_000 / 64) {
        let (l, _) = render(&mut chunked, 64);
        got.extend(l);
    }
    assert_eq!(onsets(&got), onsets(&expected));
    assert_eq!(got, expected);
}

#[test]
fn a_machine_that_was_never_started_is_silent() {
    let mut m = kick_machine(48_000.0, 120.0);
    let (l, r) = render(&mut m, 48_000);
    assert!(onsets(&l).is_empty());
    assert!(onsets(&r).is_empty());
}

#[test]
fn stop_ends_new_hits_but_lets_ringing_hits_decay_naturally() {
    let kit = kit_with(vec![(DrumRole::Kick, piece(samples(0.5, 2_000)))], 48_000);
    let mut m = DrumMachine::new(
        48_000.0,
        kit,
        groove(four_on_the_floor(DrumRole::Kick), vec![]),
        settings(120.0),
    );
    m.play();
    let (first, _) = render(&mut m, 100);
    assert!(first[0] != 0.0);
    m.stop();
    assert!(!m.is_playing());
    let (rest, _) = render(&mut m, 48_000);
    // The hit that was ringing keeps its tail...
    assert!(rest[..1_900].iter().all(|s| *s != 0.0));
    // ...but the beat at sample 24 000 never comes.
    assert!(rest[1_900..].iter().all(|s| *s == 0.0));
}

#[test]
fn a_tempo_change_keeps_the_position_in_the_bar() {
    let mut m = kick_machine(48_000.0, 120.0);
    m.play();
    // 1.5 beats at 120 bpm.
    let (head, _) = render(&mut m, 36_000);
    assert_eq!(onsets(&head), vec![0, 24_000]);
    m.set_settings(settings(60.0));
    // Half a beat left to beat 2, now at 60 bpm = 24 000 frames.
    let (tail, _) = render(&mut m, 30_000);
    assert_eq!(onsets(&tail), vec![24_000]);
}

// ── levels ─────────────────────────────────────────────────────────────────

#[test]
fn a_centered_hit_uses_the_constant_power_pan_law() {
    let mut m = kick_machine(48_000.0, 120.0);
    m.play();
    let (l, r) = render(&mut m, 10);
    assert!((l[0] - CENTER).abs() < 1e-6);
    assert!((r[0] - CENTER).abs() < 1e-6);
}

#[test]
fn a_hard_left_piece_is_silent_on_the_right() {
    let mut p = piece(impulse(1.0));
    p.pan = -1.0;
    let kit = kit_with(vec![(DrumRole::Kick, p)], 48_000);
    let mut m = DrumMachine::new(
        48_000.0,
        kit,
        groove(four_on_the_floor(DrumRole::Kick), vec![]),
        settings(120.0),
    );
    m.play();
    let (l, r) = render(&mut m, 10);
    assert!((l[0] - 1.0).abs() < 1e-6);
    assert!(r[0].abs() < 1e-6);
}

#[test]
fn velocity_scales_the_hit() {
    let kit = kit_with(vec![(DrumRole::Kick, piece(impulse(1.0)))], 48_000);
    let pattern = DrumPattern::new(4.0, vec![hit(0.0, DrumRole::Kick, 0.5)]);
    let mut m = DrumMachine::new(48_000.0, kit, groove(pattern, vec![]), settings(120.0));
    m.play();
    let (l, _) = render(&mut m, 10);
    assert!((l[0] - 0.5 * CENTER).abs() < 1e-6);
}

#[test]
fn volume_scales_the_whole_machine() {
    let mut m = kick_machine(48_000.0, 120.0);
    m.set_settings(DrumSettings {
        bpm: 120.0,
        volume: 0.5,
    });
    m.play();
    let (l, _) = render(&mut m, 10);
    assert!((l[0] - 0.5 * CENTER).abs() < 1e-6);
}

#[test]
fn the_velocity_picks_the_sample_layer() {
    let layered = DrumPiece {
        layers: vec![
            DrumLayer {
                min_velocity: 0.0,
                max_velocity: 0.5,
                samples: vec![impulse(0.25)],
                gain: 1.0,
            },
            DrumLayer {
                min_velocity: 0.5,
                max_velocity: 1.0,
                samples: vec![impulse(1.0)],
                gain: 1.0,
            },
        ],
        gain: 1.0,
        pan: 0.0,
        choke_group: None,
    };
    for (velocity, sample_value) in [(0.3f32, 0.25f32), (0.9, 1.0)] {
        let kit = kit_with(vec![(DrumRole::Kick, layered.clone())], 48_000);
        let pattern = DrumPattern::new(4.0, vec![hit(0.0, DrumRole::Kick, velocity)]);
        let mut m = DrumMachine::new(48_000.0, kit, groove(pattern, vec![]), settings(120.0));
        m.play();
        let (l, _) = render(&mut m, 10);
        let expected = sample_value * velocity * CENTER;
        assert!((l[0] - expected).abs() < 1e-6, "velocity {velocity}");
    }
}

#[test]
fn alternate_samples_in_a_layer_play_round_robin() {
    let p = DrumPiece {
        layers: vec![DrumLayer {
            min_velocity: 0.0,
            max_velocity: 1.0,
            samples: vec![impulse(1.0), impulse(0.5)],
            gain: 1.0,
        }],
        gain: 1.0,
        pan: 0.0,
        choke_group: None,
    };
    let kit = kit_with(vec![(DrumRole::Kick, p)], 48_000);
    let mut m = DrumMachine::new(
        48_000.0,
        kit,
        groove(four_on_the_floor(DrumRole::Kick), vec![]),
        settings(120.0),
    );
    m.play();
    let (l, _) = render(&mut m, 72_001);
    let levels: Vec<f32> = [0usize, 24_000, 48_000, 72_000]
        .iter()
        .map(|&i| l[i] / CENTER)
        .collect();
    for (got, want) in levels.iter().zip([1.0f32, 0.5, 1.0, 0.5]) {
        assert!((got - want).abs() < 1e-6, "levels {levels:?}");
    }
}

#[test]
fn a_closed_hat_chokes_the_ringing_open_hat() {
    let mut open = piece(samples(1.0, 96_000));
    open.choke_group = Some(1);
    let mut closed = piece(impulse(0.5));
    closed.choke_group = Some(1);
    let kit = kit_with(
        vec![(DrumRole::HatOpen, open), (DrumRole::HatClosed, closed)],
        48_000,
    );
    let pattern = DrumPattern::new(
        4.0,
        vec![
            hit(0.0, DrumRole::HatOpen, 1.0),
            hit(1.0, DrumRole::HatClosed, 1.0),
        ],
    );
    let mut m = DrumMachine::new(48_000.0, kit, groove(pattern, vec![]), settings(120.0));
    m.play();
    let (l, _) = render(&mut m, 48_000);
    assert!(l[23_999] != 0.0, "the open hat rings until the closed hat");
    assert!(
        l[26_000..].iter().all(|s| *s == 0.0),
        "the open hat is cut shortly after the closed hat"
    );
}

#[test]
fn many_simultaneous_hits_never_overflow_or_produce_non_finite_samples() {
    let kit = kit_with(vec![(DrumRole::Kick, piece(samples(0.01, 48_000)))], 48_000);
    let hits = (0..500).map(|_| hit(0.0, DrumRole::Kick, 1.0)).collect();
    let mut m = DrumMachine::new(
        48_000.0,
        kit,
        groove(DrumPattern::new(4.0, hits), vec![]),
        settings(120.0),
    );
    m.play();
    let (l, r) = render(&mut m, 48_000);
    assert!(l.iter().chain(r.iter()).all(|s| s.is_finite()));
}

// ── fills ──────────────────────────────────────────────────────────────────

fn fill_machine() -> DrumMachine {
    let kit = kit_with(
        vec![
            (DrumRole::Kick, piece(impulse(1.0))),
            (DrumRole::Snare, piece(impulse(0.5))),
        ],
        48_000,
    );
    DrumMachine::new(
        48_000.0,
        kit,
        groove(
            four_on_the_floor(DrumRole::Kick),
            vec![four_on_the_floor(DrumRole::Snare)],
        ),
        settings(120.0),
    )
}

/// The role heard at each onset: the kick impulse is 1.0, the snare 0.5.
fn onset_levels(buf: &[f32]) -> Vec<(usize, f32)> {
    onsets(buf)
        .into_iter()
        .map(|i| (i, (buf[i] / CENTER * 10.0).round() / 10.0))
        .collect()
}

#[test]
fn a_fill_takes_over_the_rest_of_the_bar_then_the_groove_returns() {
    let mut m = fill_machine();
    m.play();
    // Beat 1.5.
    let (head, _) = render(&mut m, 36_000);
    m.trigger_fill();
    assert!(m.position().in_fill);
    let (tail, _) = render(&mut m, 96_000 - 36_000 + 1);
    let mut all = head;
    all.extend(tail);
    assert_eq!(
        onset_levels(&all),
        vec![
            (0, 1.0),
            (24_000, 1.0),
            (48_000, 0.5),
            (72_000, 0.5),
            (96_000, 1.0),
        ]
    );
}

#[test]
fn a_fill_asked_for_in_the_last_beat_plays_the_whole_next_bar() {
    let mut m = fill_machine();
    m.play();
    // Beat 3.5.
    let (head, _) = render(&mut m, 84_000);
    m.trigger_fill();
    let (tail, _) = render(&mut m, 192_000 - 84_000 + 1);
    let mut all = head;
    all.extend(tail);
    assert_eq!(
        onset_levels(&all),
        vec![
            (0, 1.0),
            (24_000, 1.0),
            (48_000, 1.0),
            (72_000, 1.0),
            (96_000, 0.5),
            (120_000, 0.5),
            (144_000, 0.5),
            (168_000, 0.5),
            (192_000, 1.0),
        ]
    );
}

#[test]
fn a_groove_without_fills_ignores_the_fill_request() {
    let mut m = kick_machine(48_000.0, 120.0);
    m.play();
    m.trigger_fill();
    assert!(!m.position().in_fill);
    let (l, _) = render(&mut m, 48_001);
    assert_eq!(onsets(&l), vec![0, 24_000, 48_000]);
}

// ── position ───────────────────────────────────────────────────────────────

#[test]
fn position_reports_the_bar_and_beat_being_heard() {
    let mut m = kick_machine(48_000.0, 120.0);
    m.play();
    render(&mut m, 60_000); // beat 2.5
    let p = m.position();
    assert_eq!((p.bar, p.beat), (0, 2));
    render(&mut m, 48_000); // beat 4.5
    let p = m.position();
    assert_eq!((p.bar, p.beat), (1, 0));
}

// ── swaps ──────────────────────────────────────────────────────────────────

#[test]
fn replacing_the_kit_hands_back_the_old_one_and_cuts_its_voices() {
    let old = kit_with(vec![(DrumRole::Kick, piece(samples(1.0, 96_000)))], 48_000);
    let mut m = DrumMachine::new(
        48_000.0,
        Arc::clone(&old),
        groove(four_on_the_floor(DrumRole::Kick), vec![]),
        settings(120.0),
    );
    m.play();
    render(&mut m, 1_000);
    let new = kit_with(vec![(DrumRole::Kick, piece(impulse(1.0)))], 48_000);
    let returned = m.replace_kit(new);
    assert!(Arc::ptr_eq(&returned, &old));
    let (l, _) = render(&mut m, 22_000);
    assert!(
        l.iter().all(|s| *s == 0.0),
        "no voice of the old kit keeps sounding"
    );
}

#[test]
fn replacing_the_groove_hands_back_the_old_one_and_keeps_the_beat() {
    let mut m = kick_machine(48_000.0, 120.0);
    m.play();
    render(&mut m, 36_000); // beat 1.5
    let snare_groove = groove(four_on_the_floor(DrumRole::Snare), vec![]);
    let kit = kit_with(
        vec![
            (DrumRole::Kick, piece(impulse(1.0))),
            (DrumRole::Snare, piece(impulse(0.5))),
        ],
        48_000,
    );
    m.replace_kit(kit);
    let old = m.replace_groove(snare_groove);
    assert_eq!(old.id, "test");
    let (l, _) = render(&mut m, 12_001);
    assert_eq!(onset_levels(&l), vec![(12_000, 0.5)]);
}
