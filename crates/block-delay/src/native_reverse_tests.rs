use super::*;
use crate::dsp_probe;
use block_core::MonoProcessor;
use std::f32::consts::TAU;

const SR: f32 = 48_000.0;

/// A linear up-chirp (rising pitch). Its time-reversal is a down-chirp, so a
/// reversed echo correlates with the reversed probe, not the forward one.
fn up_chirp(len: usize) -> Vec<f32> {
    let dur = len as f32 / SR;
    let (f0, f1) = (300.0_f32, 4_000.0_f32);
    let k = (f1 - f0) / dur;
    (0..len)
        .map(|i| {
            let t = i as f32 / SR;
            (TAU * (f0 * t + 0.5 * k * t * t)).sin()
        })
        .collect()
}

/// Amplitude-normalized cross-correlation at zero lag (cosine similarity).
fn ncc(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na > 0.0 && nb > 0.0 {
        dot / (na * nb)
    } else {
        0.0
    }
}

// --- Proposal: the echo is the input segment played backwards ---

#[test]
fn reverse_echo_matches_the_time_reversed_input() {
    // The model reverses a `time_ms`-long segment. Use a short segment fully
    // filled by the chirp, fully wet, no feedback — the next segment of output
    // is exactly the reversed probe.
    let time_ms = 50.0_f32;
    let segment_len = (time_ms * 0.001 * SR).round() as usize;
    let probe = up_chirp(segment_len);
    let mut reversed_probe = probe.clone();
    reversed_probe.reverse();

    let mut input = vec![0.0; segment_len * 3];
    input[..segment_len].copy_from_slice(&probe);

    let mut delay = ReverseDelay::new(
        ReverseDelayParams {
            time_ms,
            feedback: 0.0,
            mix: 1.0,
        },
        SR,
    );
    let out = dsp_probe::render_mono(&mut delay, &input);

    // Scan for the best match against the reversed probe, and measure the
    // forward-probe match at the same place.
    let probe_len = segment_len;
    let mut best_rev = 0.0_f32;
    let mut fwd_at_best = 0.0_f32;
    let mut offset = 8;
    while offset + probe_len < out.len() {
        let seg = &out[offset..offset + probe_len];
        let rev = ncc(seg, &reversed_probe).abs();
        if rev > best_rev {
            best_rev = rev;
            fwd_at_best = ncc(seg, &probe).abs();
        }
        offset += 8;
    }

    assert!(
        best_rev > 0.4,
        "no time-reversed echo found (best reversed correlation {best_rev:.3})"
    );
    assert!(
        best_rev > fwd_at_best + 0.1,
        "echo should match the reversed input better than the forward one \
         (reversed {best_rev:.3} vs forward {fwd_at_best:.3})"
    );
}

// --- Denormal / NaN guards ---

#[test]
fn reverse_delay_outputs_finite_values() {
    let mut delay = ReverseDelay::new(ReverseDelayParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = ReverseDelay::new(ReverseDelayParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = ReverseDelay::new(ReverseDelayParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = ReverseDelay::new(ReverseDelayParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
