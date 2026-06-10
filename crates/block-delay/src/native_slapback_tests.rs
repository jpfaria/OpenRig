use super::*;
use crate::dsp_probe;
use crate::registry::native_digital_clean::{DigitalCleanDelay, DigitalCleanParams};
use block_core::MonoProcessor;

const SR: f32 = 48_000.0;

fn slapback(time_ms: f32, feedback: f32, mix: f32) -> SlapbackDelay {
    SlapbackDelay::new(
        SlapbackParams {
            time_ms,
            feedback,
            mix,
        },
        SR,
    )
}

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

// --- Proposal: a single, short, analog-flavoured slap (NOT a clean digital tap) ---

#[test]
fn slapback_is_audibly_distinct_from_digital_clean() {
    // Identical knobs: any difference must come from the model's own character.
    let input = dsp_probe::noise_burst(20_000, 4_096, 0.5, 0x51AB);
    let mut slap = slapback(110.0, 0.18, 0.5);
    let mut clean = digital_clean(110.0, 0.18, 0.5);

    let slap_out = dsp_probe::render_mono(&mut slap, &input);
    let clean_out = dsp_probe::render_mono(&mut clean, &input);

    let diff = dsp_probe::rms_difference(&slap_out, &clean_out);
    assert!(
        diff > 0.1,
        "slapback must not be a clone of digital_clean (rms diff {diff:.4})"
    );
}

#[test]
fn slapback_repeat_is_darker_than_the_dry_signal() {
    let delay_samples = (110.0_f32 * 0.001 * SR) as usize;
    let burst = 2_048usize;
    let mut slap = slapback(110.0, 0.18, 0.6);

    let input = dsp_probe::noise_burst(20_000, burst, 0.5, 0xDA12);
    let out = dsp_probe::render_mono(&mut slap, &input);

    let dry = &input[0..burst];
    let echo = &out[delay_samples..delay_samples + burst];
    let dry_centroid = dsp_probe::spectral_centroid(dry, SR);
    let echo_centroid = dsp_probe::spectral_centroid(echo, SR);

    assert!(
        echo_centroid < dry_centroid * 0.85,
        "slap repeat should roll off highs (tape/analog): dry {dry_centroid:.0} Hz, echo {echo_centroid:.0} Hz"
    );
}

#[test]
fn slapback_is_a_single_slap_not_a_wash() {
    let mut slap = slapback(110.0, 0.18, 0.5);
    let out = dsp_probe::render_mono(&mut slap, &dsp_probe::impulse(30_000));
    let peaks = dsp_probe::peaks(&out, 0.01, 2_400);

    // peaks[0] = dry, peaks[1] = the slap. The second repeat must be well down.
    assert!(peaks.len() >= 2, "expected dry + a slap");
    let slap_peak = peaks[1].1;
    let second_repeat = peaks.get(2).map(|p| p.1).unwrap_or(0.0);
    let ratio_db = 20.0 * (second_repeat / slap_peak).max(1e-6).log10();
    assert!(
        ratio_db <= -12.0,
        "second repeat should be ≥12 dB below the slap, got {ratio_db:.1} dB"
    );
}

// --- Denormal / NaN guards ---

#[test]
fn slapback_outputs_finite_values() {
    let mut delay = SlapbackDelay::new(SlapbackParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let output = delay.process_sample(0.2);
        assert!(output.is_finite());
    }
}

#[test]
fn process_frame_silence_output_is_finite() {
    let mut delay = SlapbackDelay::new(SlapbackParams::default(), 44100.0);
    for i in 0..1024 {
        let out = delay.process_sample(0.0);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_frame_sine_output_is_finite() {
    let mut delay = SlapbackDelay::new(SlapbackParams::default(), 44100.0);
    for i in 0..1024 {
        let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let out = delay.process_sample(input);
        assert!(out.is_finite(), "non-finite at sample {i}: {out}");
    }
}

#[test]
fn process_block_1024_frames_all_finite() {
    let mut delay = SlapbackDelay::new(SlapbackParams::default(), 44100.0);
    let mut buf: Vec<f32> = (0..1024)
        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    delay.process_block(&mut buf);
    for (i, &s) in buf.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at index {i}: {s}");
    }
}
