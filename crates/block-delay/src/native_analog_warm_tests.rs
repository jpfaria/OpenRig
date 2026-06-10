use super::*;
use crate::dsp_probe;
use block_core::MonoProcessor;

const SR: f32 = 48_000.0;

fn analog_warm(time_ms: f32, feedback: f32, mix: f32, tone: f32) -> AnalogWarmDelay {
    AnalogWarmDelay::new(
        AnalogWarmParams {
            time_ms,
            feedback,
            mix,
            tone,
        },
        SR,
    )
}

// --- Proposal: BBD analog — repeats darken AND carry warmth (saturation) ---

#[test]
fn analog_warm_repeats_progressively_darken() {
    let delay_samples = (120.0_f32 * 0.001 * SR) as usize;
    let burst = 2_048usize;
    let mut delay = analog_warm(120.0, 0.5, 0.6, 0.6);

    let input = dsp_probe::noise_burst(30_000, burst, 0.5, 0xA17C);
    let out = dsp_probe::render_mono(&mut delay, &input);

    let echo1 = &out[delay_samples..delay_samples + burst];
    let echo2 = &out[2 * delay_samples..2 * delay_samples + burst];
    let c1 = dsp_probe::spectral_centroid(echo1, SR);
    let c2 = dsp_probe::spectral_centroid(echo2, SR);

    assert!(
        c2 < c1 * 0.9,
        "each analog repeat must get darker: echo1 {c1:.0} Hz, echo2 {c2:.0} Hz"
    );
}

#[test]
fn analog_warm_adds_saturation_harmonics() {
    let f0 = 1_000.0;
    // Fully wet so the measurement reflects the repeat itself, not the dry input.
    let mut delay = analog_warm(60.0, 0.5, 1.0, 0.9);

    let out = dsp_probe::render_mono(&mut delay, &dsp_probe::sine(16_384, f0, SR, 0.9));
    let ratio = dsp_probe::harmonic_ratio(&out, f0, SR);

    // A linear delay leaves the fundamental alone (ratio < 0.05, see digital_clean).
    // "Warm" must add harmonic content above that floor.
    assert!(
        ratio > 0.1,
        "analog warmth must add saturation harmonics on the repeat; ratio {ratio:.4}"
    );
}

// --- Denormal / NaN guards ---

#[test]
fn analog_warm_outputs_finite_values() {
    let mut delay = AnalogWarmDelay::new(AnalogWarmParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = AnalogWarmDelay::new(AnalogWarmParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = AnalogWarmDelay::new(AnalogWarmParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = AnalogWarmDelay::new(AnalogWarmParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
