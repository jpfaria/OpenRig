use super::*;
use crate::dsp_probe;
use block_core::MonoProcessor;

const SR: f32 = 48_000.0;

/// Digital Clean built with normalized params (what `new` expects: feedback/mix
/// in 0..1, time in ms). Knobs chosen so echoes are well separated and audible.
fn digital_clean(time_ms: f32, feedback: f32, mix: f32) -> DigitalCleanDelay {
    DigitalCleanDelay::new(
        DigitalCleanParams {
            time_ms,
            feedback,
            mix,
        },
        SR,
    )
}

// --- Proposal: pristine repeats, exact timing, no colour, linear ---

#[test]
fn digital_clean_echoes_land_on_multiples_of_time_ms() {
    let time_ms = 100.0;
    let delay_samples = (time_ms * 0.001 * SR) as usize; // 4800
    let mut delay = digital_clean(time_ms, 0.6, 0.5);

    let out = dsp_probe::render_mono(&mut delay, &dsp_probe::impulse(20_000));
    let peaks = dsp_probe::peaks(&out, 0.05, delay_samples / 2);

    assert!(
        peaks.len() >= 3,
        "expected dry + at least two echoes, got {} peaks",
        peaks.len()
    );
    for pair in peaks.windows(2) {
        let gap = pair[1].0 - pair[0].0;
        let drift = gap as i64 - delay_samples as i64;
        assert!(
            drift.abs() <= 2,
            "echo spacing {gap} drifted from {delay_samples} by {drift} samples"
        );
    }
}

#[test]
fn digital_clean_decays_by_feedback_each_repeat() {
    let feedback = 0.6;
    let mut delay = digital_clean(100.0, feedback, 0.5);

    let out = dsp_probe::render_mono(&mut delay, &dsp_probe::impulse(20_000));
    let peaks = dsp_probe::peaks(&out, 0.05, 2_400);

    // peaks[0] = dry, peaks[1] = first echo, peaks[2] = second echo.
    let ratio = peaks[2].1 / peaks[1].1;
    assert!(
        (ratio - feedback).abs() < 0.05,
        "successive-echo ratio {ratio:.3} should track feedback {feedback:.3}"
    );
}

#[test]
fn digital_clean_repeats_keep_their_brightness() {
    let delay_samples = 4_800usize;
    let burst = 1_024usize;
    let mut delay = digital_clean(100.0, 0.6, 0.5);

    let input = dsp_probe::noise_burst(20_000, burst, 0.5, 0x51A7);
    let out = dsp_probe::render_mono(&mut delay, &input);

    let echo1 = &out[delay_samples..delay_samples + burst];
    let echo2 = &out[2 * delay_samples..2 * delay_samples + burst];
    let c1 = dsp_probe::spectral_centroid(echo1, SR);
    let c2 = dsp_probe::spectral_centroid(echo2, SR);

    assert!(c1 > 1_000.0, "noise echo should be broadband, centroid {c1:.0} Hz");
    assert!(
        (c2 - c1).abs() / c1 < 0.05,
        "clean repeats must not darken: c1={c1:.0} Hz, c2={c2:.0} Hz"
    );
}

#[test]
fn digital_clean_stays_linear_under_hot_input() {
    let f0 = 1_000.0;
    let mut delay = digital_clean(100.0, 0.6, 0.5);

    let out = dsp_probe::render_mono(&mut delay, &dsp_probe::sine(8_192, f0, SR, 0.9));
    let ratio = dsp_probe::harmonic_ratio(&out, f0, SR);

    assert!(
        ratio < 0.05,
        "clean delay must not add harmonics (saturation); ratio {ratio:.4}"
    );
}

// --- Denormal / NaN guards (cheap, kept from the original suite) ---

#[test]
fn digital_clean_outputs_finite_values() {
    let mut delay = DigitalCleanDelay::new(DigitalCleanParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = DigitalCleanDelay::new(DigitalCleanParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = DigitalCleanDelay::new(DigitalCleanParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = DigitalCleanDelay::new(DigitalCleanParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
