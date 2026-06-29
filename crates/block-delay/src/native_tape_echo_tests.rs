use super::*;
use crate::dsp_probe;
use crate::registry::native_tape_vintage::{TapeVintageDelay, TapeVintageParams};
use block_core::MonoProcessor;

const SR: f32 = 48_000.0;

fn tape_echo(time_ms: f32, feedback: f32, mix: f32, flutter: f32) -> TapeEchoDelay {
    TapeEchoDelay::new(
        TapeEchoParams {
            time_ms,
            feedback,
            mix,
            flutter,
        },
        SR,
    )
}

// --- Proposal: hot tape echo — random-walk wow/flutter + heavy magnetic
//     saturation + tape tone; a distinct voice from the subtle Tape Vintage ---

#[test]
fn tape_echo_is_distinct_from_tape_vintage() {
    let input = dsp_probe::noise_burst(20_000, 4_096, 0.5, 0x7A0E);
    let mut echo = tape_echo(260.0, 0.4, 0.5, 0.6);
    let mut vintage = TapeVintageDelay::new(
        TapeVintageParams {
            time_ms: 260.0,
            feedback: 0.4,
            mix: 0.5,
            tone: 0.5,
            flutter: 0.6,
        },
        SR,
    );
    let echo_out = dsp_probe::render_mono(&mut echo, &input);
    let vintage_out = dsp_probe::render_mono(&mut vintage, &input);

    let diff = dsp_probe::rms_difference(&echo_out, &vintage_out);
    assert!(
        diff > 0.1,
        "Tape Echo must voice differently from Tape Vintage (rms diff {diff:.4})"
    );
}

#[test]
fn tape_echo_flutter_modulates_the_repeat() {
    let input = dsp_probe::sine(16_384, 1_000.0, SR, 0.6);
    let mut steady = tape_echo(80.0, 0.4, 1.0, 0.0);
    let mut wobbled = tape_echo(80.0, 0.4, 1.0, 1.0);
    let steady_out = dsp_probe::render_mono(&mut steady, &input);
    let wobbled_out = dsp_probe::render_mono(&mut wobbled, &input);

    let diff = dsp_probe::rms_difference(&wobbled_out, &steady_out);
    assert!(
        diff > 0.05,
        "flutter must modulate the repeat vs a steady tape (rms diff {diff:.4})"
    );
}

#[test]
fn tape_echo_repeat_is_darker_than_dry() {
    let delay_samples = (80.0_f32 * 0.001 * SR) as usize;
    let burst = 2_048usize;
    let mut delay = tape_echo(80.0, 0.4, 0.6, 0.0);

    let input = dsp_probe::noise_burst(30_000, burst, 0.5, 0x7EC0);
    let out = dsp_probe::render_mono(&mut delay, &input);

    let dry = &input[0..burst];
    let echo = &out[delay_samples..delay_samples + burst];
    let dry_c = dsp_probe::spectral_centroid(dry, SR);
    let echo_c = dsp_probe::spectral_centroid(echo, SR);

    assert!(
        echo_c < dry_c * 0.9,
        "tape tone should roll off highs: dry {dry_c:.0} Hz, echo {echo_c:.0} Hz"
    );
}

#[test]
fn tape_echo_adds_heavy_saturation() {
    let f0 = 1_000.0;
    let mut delay = tape_echo(80.0, 0.5, 1.0, 0.0);

    let out = dsp_probe::render_mono(&mut delay, &dsp_probe::sine(16_384, f0, SR, 0.9));
    let ratio = dsp_probe::harmonic_ratio(&out, f0, SR);

    assert!(
        ratio > 0.15,
        "tape echo must add strong magnetic saturation; ratio {ratio:.4}"
    );
}

// --- Denormal / NaN guards ---

#[test]
fn tape_echo_outputs_finite_values() {
    let mut delay = TapeEchoDelay::new(TapeEchoParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = TapeEchoDelay::new(TapeEchoParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = TapeEchoDelay::new(TapeEchoParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = TapeEchoDelay::new(TapeEchoParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
