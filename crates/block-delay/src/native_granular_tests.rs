use super::*;
use crate::dsp_probe;
use crate::registry::native_digital_clean::{DigitalCleanDelay, DigitalCleanParams};
use block_core::MonoProcessor;

const SR: f32 = 48_000.0;

fn granular(time_ms: f32, feedback: f32, mix: f32, spread: f32) -> GranularDelay {
    GranularDelay::new(
        GranularParams {
            time_ms,
            feedback,
            mix,
            spread,
        },
        SR,
    )
}

/// Number of samples whose magnitude exceeds `thresh` within `range`.
fn energy_width(signal: &[f32], range: std::ops::Range<usize>, thresh: f32) -> usize {
    signal[range].iter().filter(|&&s| s.abs() > thresh).count()
}

// --- Proposal: granular — the echo is a smeared cloud of windowed grains,
//     not a sharp repeat ---

#[test]
fn granular_smears_a_burst_into_a_cloud() {
    let time_ms = 200.0;
    let base = (time_ms * 0.001 * SR) as usize; // 9_600
    let mut delay = granular(time_ms, 0.2, 1.0, 0.8);

    // A short burst, then silence.
    let input = dsp_probe::noise_burst(40_000, 512, 0.5, 0x6841);
    let out = dsp_probe::render_mono(&mut delay, &input);
    let width = energy_width(&out, base..base + 8_000, 0.02);

    // A plain delay would echo the 512-sample burst once (~512 active samples);
    // granular re-scatters it into a much wider cloud.
    assert!(
        width > 1_500,
        "granular echo must be a spread cloud, got only {width} active samples"
    );
}

#[test]
fn granular_is_distinct_from_a_plain_delay() {
    let input = dsp_probe::noise_burst(20_000, 4_096, 0.5, 0x6841);
    let mut gran = granular(250.0, 0.3, 0.5, 0.6);
    let mut clean = DigitalCleanDelay::new(
        DigitalCleanParams {
            time_ms: 250.0,
            feedback: 0.3,
            mix: 0.5,
        },
        SR,
    );
    let gran_out = dsp_probe::render_mono(&mut gran, &input);
    let clean_out = dsp_probe::render_mono(&mut clean, &input);

    let diff = dsp_probe::rms_difference(&gran_out, &clean_out);
    assert!(
        diff > 0.1,
        "granular must not equal a plain delay (rms diff {diff:.4})"
    );
}

// --- Denormal / NaN guards ---

#[test]
fn granular_outputs_finite_values() {
    let mut delay = GranularDelay::new(GranularParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = GranularDelay::new(GranularParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = GranularDelay::new(GranularParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = GranularDelay::new(GranularParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
