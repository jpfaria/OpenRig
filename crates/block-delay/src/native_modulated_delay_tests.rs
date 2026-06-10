use super::*;
use crate::dsp_probe;
use crate::registry::native_digital_clean::{DigitalCleanDelay, DigitalCleanParams};
use block_core::MonoProcessor;

const SR: f32 = 48_000.0;

fn modulated(time_ms: f32, feedback: f32, mix: f32, rate_hz: f32, depth: f32) -> ModulatedDelay {
    ModulatedDelay::new(
        ModulatedDelayParams {
            time_ms,
            feedback,
            mix,
            rate_hz,
            depth,
        },
        SR,
    )
}

// --- Proposal: LFO-modulated delay time (chorus/vibrato in the repeats) ---

#[test]
fn modulated_depth_actually_modulates_the_repeat() {
    let input = dsp_probe::sine(16_384, 1_000.0, SR, 0.6);

    let mut flat = modulated(120.0, 0.4, 1.0, 1.5, 0.0);
    let mut deep = modulated(120.0, 0.4, 1.0, 1.5, 0.9);
    let flat_out = dsp_probe::render_mono(&mut flat, &input);
    let deep_out = dsp_probe::render_mono(&mut deep, &input);

    let diff = dsp_probe::rms_difference(&deep_out, &flat_out);
    assert!(
        diff > 0.05,
        "depth must modulate the delay time vs depth=0 (rms diff {diff:.4})"
    );
}

#[test]
fn modulated_is_distinct_from_a_static_delay() {
    let input = dsp_probe::noise_burst(20_000, 4_096, 0.5, 0x303D);

    let mut modd = modulated(120.0, 0.4, 0.5, 1.5, 0.9);
    let mut clean = DigitalCleanDelay::new(
        DigitalCleanParams {
            time_ms: 120.0,
            feedback: 0.4,
            mix: 0.5,
        },
        SR,
    );
    let modd_out = dsp_probe::render_mono(&mut modd, &input);
    let clean_out = dsp_probe::render_mono(&mut clean, &input);

    let diff = dsp_probe::rms_difference(&modd_out, &clean_out);
    assert!(
        diff > 0.05,
        "a modulated delay must not equal a static one (rms diff {diff:.4})"
    );
}

// --- Denormal / NaN guards ---

#[test]
fn modulated_delay_outputs_finite_values() {
    let mut delay = ModulatedDelay::new(ModulatedDelayParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = ModulatedDelay::new(ModulatedDelayParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = ModulatedDelay::new(ModulatedDelayParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = ModulatedDelay::new(ModulatedDelayParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
