use super::*;
use crate::dsp_probe;
use block_core::MonoProcessor;

const SR: f32 = 48_000.0;

fn rhythmic(time_ms: f32, feedback: f32, mix: f32, subdivision: f32) -> RhythmicDelay {
    RhythmicDelay::new(
        RhythmicParams {
            time_ms,
            feedback,
            mix,
            subdivision,
        },
        SR,
    )
}

fn first_echo_index(delay: &mut RhythmicDelay) -> usize {
    let out = dsp_probe::render_mono(delay, &dsp_probe::impulse(40_000));
    // mix=1.0 suppresses the dry, so the first peak is the echo.
    dsp_probe::peaks(&out, 0.05, 1_000)[0].0
}

// --- Proposal: echoes fall on a syncopated grid (dotted-eighth / triplet),
//     NOT on the straight beat ---

#[test]
fn dotted_eighth_places_the_echo_at_three_quarters_of_the_beat() {
    let beat_ms = 400.0;
    let mut delay = rhythmic(beat_ms, 0.0, 1.0, 0.0); // subdivision 0 = dotted-8th
    let echo = first_echo_index(&mut delay);

    let expected = (beat_ms * 0.75 * 0.001 * SR) as i64; // 14_400
    let straight = (beat_ms * 0.001 * SR) as i64; // 19_200
    assert!(
        (echo as i64 - expected).abs() <= 30,
        "dotted-eighth echo should land at 0.75·beat ({expected}), got {echo}"
    );
    assert!(
        (echo as i64 - straight).abs() > 1_000,
        "must NOT be a straight quarter-note delay"
    );
}

#[test]
fn triplet_places_the_echo_at_two_thirds_of_the_beat() {
    let beat_ms = 600.0;
    let mut delay = rhythmic(beat_ms, 0.0, 1.0, 1.0); // subdivision 1 = triplet
    let echo = first_echo_index(&mut delay);

    let expected = (beat_ms * (2.0 / 3.0) * 0.001 * SR) as i64; // ~19_200
    assert!(
        (echo as i64 - expected).abs() <= 30,
        "triplet echo should land at (2/3)·beat ({expected}), got {echo}"
    );
}

#[test]
fn subdivision_changes_the_echo_timing() {
    let beat_ms = 480.0;
    let mut dotted = rhythmic(beat_ms, 0.0, 1.0, 0.0);
    let mut triplet = rhythmic(beat_ms, 0.0, 1.0, 1.0);
    let a = first_echo_index(&mut dotted);
    let b = first_echo_index(&mut triplet);
    assert!(
        a.abs_diff(b) > 1_000,
        "different subdivisions must place the echo at different times ({a} vs {b})"
    );
}

// --- Denormal / NaN guards ---

#[test]
fn rhythmic_outputs_finite_values() {
    let mut delay = RhythmicDelay::new(RhythmicParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = RhythmicDelay::new(RhythmicParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = RhythmicDelay::new(RhythmicParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = RhythmicDelay::new(RhythmicParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
