
use super::*;
use std::f32::consts::TAU;

#[test]
fn silence_in_silence_out() {
    let mut svf = Svf::new(1_000.0, 2.0, 44_100.0);
    for _ in 0..2048 {
        let f = svf.process(0.0);
        assert_eq!(f.low, 0.0);
        assert_eq!(f.band, 0.0);
        assert_eq!(f.high, 0.0);
    }
}

#[test]
fn sine_input_finite() {
    let mut svf = Svf::new(1_000.0, 2.0, 44_100.0);
    let sr = 44_100.0_f32;
    for i in 0..4096 {
        let x = (TAU * 440.0 * i as f32 / sr).sin();
        let f = svf.process(x);
        assert!(f.low.is_finite() && f.band.is_finite() && f.high.is_finite());
    }
}

#[test]
fn lowpass_attenuates_above_cutoff() {
    // Sine at 5 kHz through 200 Hz LP: low-pass output should be
    // much smaller than input.
    let mut svf = Svf::new(200.0, 0.7, 44_100.0);
    let sr = 44_100.0_f32;
    // Warm-up.
    for n in 0..8192 {
        let x = (TAU * 5_000.0 * n as f32 / sr).sin();
        svf.process(x);
    }
    let mut peak_in = 0.0_f32;
    let mut peak_out = 0.0_f32;
    for n in 8192..16_384 {
        let x = (TAU * 5_000.0 * n as f32 / sr).sin();
        let lp = svf.process_low(x);
        peak_in = peak_in.max(x.abs());
        peak_out = peak_out.max(lp.abs());
    }
    assert!(
        peak_out < 0.1 * peak_in,
        "LP didn't attenuate: out {peak_out} vs in {peak_in}"
    );
}

#[test]
fn bandpass_resonates_at_cutoff() {
    // Sine at cutoff should have BP magnitude approximately ~1
    // (proportional to Q in this normalized form).
    let mut svf = Svf::new(1_000.0, 2.0, 44_100.0);
    let sr = 44_100.0_f32;
    // Warm-up.
    for n in 0..8192 {
        let x = (TAU * 1_000.0 * n as f32 / sr).sin();
        svf.process(x);
    }
    let mut peak = 0.0_f32;
    for n in 8192..16_384 {
        let x = (TAU * 1_000.0 * n as f32 / sr).sin();
        let bp = svf.process_band(x);
        peak = peak.max(bp.abs());
    }
    // Q=2 → BP peak gain ~Q/2 = 1.0 in this normalisation.
    assert!(peak > 0.5, "BP didn't resonate: peak {peak}");
}

#[test]
fn coefficient_sweep_remains_stable() {
    // Sweep cutoff while feeding white-ish noise, ensure output stays bounded.
    let mut svf = Svf::new(500.0, 5.0, 44_100.0);
    let sr = 44_100.0_f32;
    for n in 0..44_100 {
        let cutoff = 100.0 + 4_000.0 * (0.5 + 0.5 * (TAU * 0.5 * n as f32 / sr).sin());
        svf.set_cutoff_q(cutoff, 5.0);
        let x = ((n as f32 * 17.0).sin()).clamp(-1.0, 1.0);
        let f = svf.process(x);
        assert!(f.band.abs() < 50.0, "diverged: {} at {n}", f.band);
    }
}
