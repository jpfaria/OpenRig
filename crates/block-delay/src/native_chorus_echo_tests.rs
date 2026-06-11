use super::*;
use crate::dsp_probe;
use crate::registry::native_bbd::{BbdDelay, BbdParams};
use block_core::MonoProcessor;

const SR: f32 = 48_000.0;

fn chorus_echo(time_ms: f32, feedback: f32, mix: f32, depth: f32) -> ChorusEchoDelay {
    ChorusEchoDelay::new(
        ChorusEchoParams {
            time_ms,
            feedback,
            mix,
            depth,
        },
        SR,
    )
}

// --- Proposal: BBD-flavoured echo with internal chorus on the repeats ---

#[test]
fn chorus_echo_depth_modulates_the_repeat() {
    let input = dsp_probe::sine(16_384, 1_000.0, SR, 0.6);
    let mut flat = chorus_echo(300.0, 0.4, 1.0, 0.0);
    let mut deep = chorus_echo(300.0, 0.4, 1.0, 0.9);
    let flat_out = dsp_probe::render_mono(&mut flat, &input);
    let deep_out = dsp_probe::render_mono(&mut deep, &input);

    let diff = dsp_probe::rms_difference(&deep_out, &flat_out);
    assert!(
        diff > 0.05,
        "chorus depth must modulate the repeat (rms diff {diff:.4})"
    );
}

#[test]
fn chorus_echo_is_distinct_from_plain_bbd() {
    let input = dsp_probe::noise_burst(20_000, 4_096, 0.5, 0xC0E0);
    let mut chorus = chorus_echo(300.0, 0.4, 0.5, 0.9);
    let mut bbd = BbdDelay::new(
        BbdParams {
            time_ms: 300.0,
            feedback: 0.4,
            mix: 0.5,
            tone: 0.5,
        },
        SR,
    );
    let chorus_out = dsp_probe::render_mono(&mut chorus, &input);
    let bbd_out = dsp_probe::render_mono(&mut bbd, &input);

    let diff = dsp_probe::rms_difference(&chorus_out, &bbd_out);
    assert!(
        diff > 0.05,
        "the chorus must make it differ from a plain BBD echo (rms diff {diff:.4})"
    );
}

#[test]
fn chorus_echo_repeat_is_darker_than_dry() {
    let delay_samples = (120.0_f32 * 0.001 * SR) as usize;
    let burst = 2_048usize;
    let mut delay = chorus_echo(120.0, 0.4, 0.6, 0.0);

    let input = dsp_probe::noise_burst(30_000, burst, 0.5, 0xC0DA);
    let out = dsp_probe::render_mono(&mut delay, &input);

    let dry = &input[0..burst];
    let echo = &out[delay_samples..delay_samples + burst];
    let dry_c = dsp_probe::spectral_centroid(dry, SR);
    let echo_c = dsp_probe::spectral_centroid(echo, SR);

    assert!(
        echo_c < dry_c * 0.9,
        "the echo should be darker than the dry: dry {dry_c:.0} Hz, echo {echo_c:.0} Hz"
    );
}

#[test]
fn chorus_echo_adds_saturation() {
    let f0 = 1_000.0;
    let mut delay = chorus_echo(80.0, 0.5, 1.0, 0.0);

    let out = dsp_probe::render_mono(&mut delay, &dsp_probe::sine(16_384, f0, SR, 0.9));
    let ratio = dsp_probe::harmonic_ratio(&out, f0, SR);

    assert!(
        ratio > 0.1,
        "the analog echo must add saturation harmonics; ratio {ratio:.4}"
    );
}

// --- Denormal / NaN guards ---

#[test]
fn chorus_echo_outputs_finite_values() {
    let mut delay = ChorusEchoDelay::new(ChorusEchoParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = ChorusEchoDelay::new(ChorusEchoParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = ChorusEchoDelay::new(ChorusEchoParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = ChorusEchoDelay::new(ChorusEchoParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
