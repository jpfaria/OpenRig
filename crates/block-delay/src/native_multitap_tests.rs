use super::*;
use crate::dsp_probe;
use block_core::MonoProcessor;

const SR: f32 = 48_000.0;

fn multitap(time_ms: f32, feedback: f32, mix: f32, taps: f32) -> MultiTapDelay {
    MultiTapDelay::new(
        MultiTapParams {
            time_ms,
            feedback,
            mix,
            taps,
        },
        SR,
    )
}

// --- Proposal: several rhythmic taps at sub-divisions of the base time ---

#[test]
fn multitap_produces_one_echo_per_tap() {
    let taps = 4.0;
    // Fully wet, no feedback: the impulse response is exactly the tap pattern.
    let mut delay = multitap(400.0, 0.0, 1.0, taps);
    let out = dsp_probe::render_mono(&mut delay, &dsp_probe::impulse(30_000));

    let peaks = dsp_probe::peaks(&out, 0.02, 1_000);
    assert!(
        peaks.len() >= taps as usize,
        "expected one echo per tap ({} taps), found {} peaks",
        taps as usize,
        peaks.len()
    );
}

#[test]
fn multitap_taps_land_on_even_sub_divisions() {
    let time_ms = 400.0;
    let taps = 4.0;
    let step = (time_ms / taps * 0.001 * SR) as i64; // 100 ms => 4800 samples
    let mut delay = multitap(time_ms, 0.0, 1.0, taps);
    let out = dsp_probe::render_mono(&mut delay, &dsp_probe::impulse(30_000));

    let peaks = dsp_probe::peaks(&out, 0.02, 1_000);
    assert!(
        peaks.len() >= 4,
        "need the full tap pattern, got {}",
        peaks.len()
    );
    for pair in peaks.windows(2) {
        let gap = pair[1].0 as i64 - pair[0].0 as i64;
        assert!(
            (gap - step).abs() <= 3,
            "taps should be evenly spaced by {step} samples, saw {gap}"
        );
    }
}

#[test]
fn multitap_is_more_than_a_single_repeat() {
    // A plain delay yields one repeat (then feedback copies). Multi-tap yields
    // several distinct echoes within the first base-time window.
    let time_ms = 400.0;
    let mut delay = multitap(time_ms, 0.0, 1.0, 4.0);
    let out = dsp_probe::render_mono(&mut delay, &dsp_probe::impulse(30_000));

    let window = (time_ms * 0.001 * SR) as usize; // 19_200 samples
    let peaks = dsp_probe::peaks(&out[..window + 1], 0.02, 1_000);
    assert!(
        peaks.len() >= 3,
        "multi-tap must place several echoes inside one base-time window, got {}",
        peaks.len()
    );
}

// --- Denormal / NaN guards ---

#[test]
fn multitap_outputs_finite_values() {
    let mut delay = MultiTapDelay::new(MultiTapParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = MultiTapDelay::new(MultiTapParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = MultiTapDelay::new(MultiTapParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = MultiTapDelay::new(MultiTapParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
