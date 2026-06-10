use super::*;
use crate::dsp_probe;
use block_core::MonoProcessor;

const SR: f32 = 48_000.0;

fn tape(time_ms: f32, feedback: f32, mix: f32, tone: f32, flutter: f32) -> TapeVintageDelay {
    TapeVintageDelay::new(
        TapeVintageParams {
            time_ms,
            feedback,
            mix,
            tone,
            flutter,
        },
        SR,
    )
}

// --- Proposal: tape — wow/flutter pitch wobble + tape tone + magnetic saturation ---

#[test]
fn tape_flutter_modulates_the_repeat() {
    let f0 = 1_000.0;
    let input = dsp_probe::sine(16_384, f0, SR, 0.6);

    let mut steady = tape(80.0, 0.4, 1.0, 0.6, 0.0);
    let mut wobbled = tape(80.0, 0.4, 1.0, 0.6, 0.9);
    let steady_out = dsp_probe::render_mono(&mut steady, &input);
    let wobbled_out = dsp_probe::render_mono(&mut wobbled, &input);

    let diff = dsp_probe::rms_difference(&wobbled_out, &steady_out);
    assert!(
        diff > 0.05,
        "flutter must audibly modulate the repeat vs a steady tape (rms diff {diff:.4})"
    );
}

#[test]
fn tape_repeat_is_darker_than_the_dry_signal() {
    let delay_samples = (80.0_f32 * 0.001 * SR) as usize;
    let burst = 2_048usize;
    let mut delay = tape(80.0, 0.4, 0.6, 0.5, 0.2);

    let input = dsp_probe::noise_burst(30_000, burst, 0.5, 0x7A9E);
    let out = dsp_probe::render_mono(&mut delay, &input);

    let dry = &input[0..burst];
    let echo = &out[delay_samples..delay_samples + burst];
    let dry_centroid = dsp_probe::spectral_centroid(dry, SR);
    let echo_centroid = dsp_probe::spectral_centroid(echo, SR);

    assert!(
        echo_centroid < dry_centroid * 0.9,
        "tape tone should roll off highs: dry {dry_centroid:.0} Hz, echo {echo_centroid:.0} Hz"
    );
}

#[test]
fn tape_adds_magnetic_saturation() {
    let f0 = 1_000.0;
    // Fully wet, low flutter so the harmonics reflect saturation, not modulation.
    let mut delay = tape(80.0, 0.5, 1.0, 0.9, 0.0);

    let out = dsp_probe::render_mono(&mut delay, &dsp_probe::sine(16_384, f0, SR, 0.9));
    let ratio = dsp_probe::harmonic_ratio(&out, f0, SR);

    assert!(
        ratio > 0.1,
        "tape must add magnetic saturation harmonics on the repeat; ratio {ratio:.4}"
    );
}

// --- Denormal / NaN guards ---

#[test]
fn tape_vintage_outputs_finite_values() {
    let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
