
use super::*;
use std::f32::consts::TAU;

#[test]
fn silence_in_silence_out() {
    let mut h = HilbertIir::new();
    for _ in 0..2048 {
        let [r, i] = h.process(0.0);
        assert_eq!(r, 0.0);
        assert_eq!(i, 0.0);
    }
}

#[test]
fn sine_input_finite() {
    let mut h = HilbertIir::new();
    let sr = 44_100.0_f32;
    for n in 0..4096 {
        let x = (TAU * 440.0 * n as f32 / sr).sin();
        let [r, i] = h.process(x);
        assert!(r.is_finite() && i.is_finite(), "non-finite at {n}");
    }
}

#[test]
fn quadrature_at_1khz_unit_circle() {
    // For a unit-amplitude sine, the analytic signal magnitude
    // should be ≈ 1 (real² + imag² = 1) once the filter is warm.
    let mut h = HilbertIir::new();
    let sr = 44_100.0_f32;
    let f = 1_000.0_f32;
    // Warm-up.
    for n in 0..8192 {
        let x = (TAU * f * n as f32 / sr).sin();
        h.process(x);
    }
    let mut max_err = 0.0_f32;
    for n in 8192..16_384 {
        let x = (TAU * f * n as f32 / sr).sin();
        let [r, i] = h.process(x);
        let mag = (r * r + i * i).sqrt();
        let err = (mag - 1.0).abs();
        if err > max_err {
            max_err = err;
        }
    }
    assert!(max_err < 0.1, "magnitude error {max_err} > 0.1");
}
