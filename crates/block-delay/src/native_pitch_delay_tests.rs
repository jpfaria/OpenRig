use super::*;
use block_core::MonoProcessor;
use std::f32::consts::TAU;

const SR: f32 = 48_000.0;

fn pitch_delay(time_ms: f32, feedback: f32, mix: f32, semitones: f32) -> PitchDelay {
    PitchDelay::new(
        PitchDelayParams {
            time_ms,
            feedback,
            mix,
            semitones,
        },
        SR,
    )
}

fn render(delay: &mut PitchDelay, input: &[f32]) -> Vec<f32> {
    input.iter().map(|&s| delay.process_sample(s)).collect()
}

fn sine(len: usize, freq: f32, amp: f32) -> Vec<f32> {
    (0..len)
        .map(|i| (i as f32 / SR * freq * TAU).sin() * amp)
        .collect()
}

/// Single-bin DFT magnitude — energy at `freq` in `signal`.
fn tone_energy(signal: &[f32], freq: f32) -> f32 {
    let mut re = 0.0;
    let mut im = 0.0;
    for (n, &s) in signal.iter().enumerate() {
        let ph = TAU * freq * n as f32 / SR;
        re += s * ph.cos();
        im -= s * ph.sin();
    }
    (re * re + im * im).sqrt() / signal.len() as f32
}

// --- Proposal: the repeats are pitch-shifted (harmonized / shimmer delay) ---

#[test]
fn pitch_delay_shifts_the_repeat_up_an_octave() {
    let f0 = 500.0;
    let mut delay = pitch_delay(120.0, 0.0, 1.0, 12.0); // +12 st = octave up
    let out = render(&mut delay, &sine(24_000, f0, 0.6));

    // Settle past the delay + shifter latency, then measure.
    let tail = &out[12_000..];
    let e_orig = tone_energy(tail, f0);
    let e_octave = tone_energy(tail, 2.0 * f0);

    assert!(
        e_octave > e_orig * 2.0,
        "the repeat should be shifted up an octave: e(500Hz)={e_orig:.4}, e(1000Hz)={e_octave:.4}"
    );
}

#[test]
fn pitch_delay_is_distinct_from_a_plain_delay() {
    let f0 = 500.0;
    let mut shifted = pitch_delay(120.0, 0.0, 1.0, 12.0);
    let mut unshifted = pitch_delay(120.0, 0.0, 1.0, 0.0); // 0 st = no shift
    let input = sine(24_000, f0, 0.6);
    let a = render(&mut shifted, &input);
    let b = render(&mut unshifted, &input);

    let tail_a = &a[12_000..];
    let tail_b = &b[12_000..];
    let octave_shifted = tone_energy(tail_a, 2.0 * f0);
    let octave_unshifted = tone_energy(tail_b, 2.0 * f0);

    assert!(
        octave_shifted > octave_unshifted * 4.0,
        "pitch shift must add octave energy a plain delay does not: {octave_shifted:.4} vs {octave_unshifted:.4}"
    );
}

// --- Denormal / NaN guards ---

#[test]
fn pitch_delay_outputs_finite_values() {
    let mut delay = PitchDelay::new(PitchDelayParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = PitchDelay::new(PitchDelayParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = PitchDelay::new(PitchDelayParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = PitchDelay::new(PitchDelayParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
