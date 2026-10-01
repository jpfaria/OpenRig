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

/// #328: a chain split aligns its paths with what a block reports, so the
/// round trip must match the delay the impulse actually comes out with.
#[test]
fn round_trip_latency_matches_the_measured_impulse_delay() {
    let mut os = Oversampler2x::new();
    let response: Vec<f32> = (0..64)
        .map(|n| {
            let [a, b] = os.up(if n == 0 { 1.0 } else { 0.0 });
            os.down([a, b])
        })
        .collect();
    let energy: f32 = response.iter().map(|v| v * v).sum();
    let centroid: f32 = response
        .iter()
        .enumerate()
        .map(|(n, v)| n as f32 * v * v)
        .sum::<f32>()
        / energy;
    let reported = Oversampler2x::new().round_trip_latency_samples() as f32;
    assert!(
        (reported - centroid).abs() <= 0.5 + 1e-3,
        "reported {reported} samples, the impulse comes out at {centroid:.2}"
    );
    assert_eq!(Oversampler2x::new().round_trip_latency_samples(), 15);
}
