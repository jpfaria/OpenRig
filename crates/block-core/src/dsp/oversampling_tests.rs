
use super::*;

#[test]
fn passthrough_is_finite() {
    let mut os = Oversampler2x::new();
    let sr = 44_100.0_f32;
    for i in 0..1024 {
        let x = (TAU * 440.0 * i as f32 / sr).sin();
        let [a, b] = os.up(x);
        let y = os.down([a, b]);
        assert!(y.is_finite(), "non-finite at {i}");
    }
}

#[test]
fn silence_in_silence_out() {
    let mut os = Oversampler2x::new();
    for _ in 0..1024 {
        let [a, b] = os.up(0.0);
        let y = os.down([a, b]);
        assert_eq!(y, 0.0);
    }
}

#[test]
fn dc_gain_close_to_unity_after_warmup() {
    let mut os = Oversampler2x::new();
    // Warm up the filter with DC.
    for _ in 0..200 {
        let [a, b] = os.up(1.0);
        os.down([a, b]);
    }
    // Now measure.
    let mut acc = 0.0;
    let n = 200;
    for _ in 0..n {
        let [a, b] = os.up(1.0);
        acc += os.down([a, b]);
    }
    let avg = acc / n as f32;
    assert!((avg - 1.0).abs() < 0.05, "DC gain {avg} not ~1.0");
}

#[test]
fn latency_is_constant() {
    let os = Oversampler2x::new();
    assert_eq!(os.latency_samples(), 7);
}
