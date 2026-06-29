use super::*;
use crate::dsp_probe;
use crate::registry::native_analog_warm::{AnalogWarmDelay, AnalogWarmParams};
use block_core::MonoProcessor;

const SR: f32 = 48_000.0;

fn bbd(time_ms: f32, feedback: f32, mix: f32, tone: f32) -> BbdDelay {
    BbdDelay::new(
        BbdParams {
            time_ms,
            feedback,
            mix,
            tone,
        },
        SR,
    )
}

// --- Proposal: bucket-brigade — steep multi-pole HF loss + saturation, a
//     distinct voice from the generic Analog Warm ---

#[test]
fn bbd_is_distinct_from_analog_warm() {
    let input = dsp_probe::noise_burst(20_000, 4_096, 0.5, 0xB1D7);
    let mut bbd = bbd(300.0, 0.4, 0.5, 0.5);
    let mut warm = AnalogWarmDelay::new(
        AnalogWarmParams {
            time_ms: 300.0,
            feedback: 0.4,
            mix: 0.5,
            tone: 0.5,
        },
        SR,
    );
    let bbd_out = dsp_probe::render_mono(&mut bbd, &input);
    let warm_out = dsp_probe::render_mono(&mut warm, &input);

    let diff = dsp_probe::rms_difference(&bbd_out, &warm_out);
    assert!(
        diff > 0.1,
        "BBD must voice differently from Analog Warm (rms diff {diff:.4})"
    );
}

#[test]
fn bbd_repeats_darken_steeply() {
    let delay_samples = (300.0_f32 * 0.001 * SR) as usize;
    let burst = 2_048usize;
    let mut delay = bbd(300.0, 0.5, 0.6, 0.5);

    let input = dsp_probe::noise_burst(40_000, burst, 0.5, 0xBD51);
    let out = dsp_probe::render_mono(&mut delay, &input);

    let echo1 = &out[delay_samples..delay_samples + burst];
    let echo2 = &out[2 * delay_samples..2 * delay_samples + burst];
    let c1 = dsp_probe::spectral_centroid(echo1, SR);
    let c2 = dsp_probe::spectral_centroid(echo2, SR);

    assert!(
        c2 < c1 * 0.8,
        "BBD repeats should lose highs steeply: echo1 {c1:.0} Hz, echo2 {c2:.0} Hz"
    );
}

#[test]
fn bbd_adds_saturation() {
    let f0 = 1_000.0;
    let mut delay = bbd(80.0, 0.5, 1.0, 0.9);

    let out = dsp_probe::render_mono(&mut delay, &dsp_probe::sine(16_384, f0, SR, 0.9));
    let ratio = dsp_probe::harmonic_ratio(&out, f0, SR);

    assert!(
        ratio > 0.1,
        "BBD must add analog saturation harmonics on the repeat; ratio {ratio:.4}"
    );
}

// --- Denormal / NaN guards ---

#[test]
fn bbd_outputs_finite_values() {
    let mut delay = BbdDelay::new(BbdParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = BbdDelay::new(BbdParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = BbdDelay::new(BbdParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = BbdDelay::new(BbdParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
