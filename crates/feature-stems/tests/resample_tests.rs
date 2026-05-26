//! RED-first tests for the resample stage.

const TARGET_SR: u32 = 44_100;

fn sine_stereo(sr: u32, secs: u32, freq_hz: f32) -> Vec<f32> {
    let frames = (sr * secs) as usize;
    let mut out = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let t = frame as f32 / sr as f32;
        let s = (t * freq_hz * std::f32::consts::TAU).sin() * 0.5;
        out.push(s);
        out.push(s);
    }
    out
}

#[test]
fn resample_same_rate_is_passthrough_with_tolerance() {
    let input = sine_stereo(TARGET_SR, 1, 440.0);
    let output = feature_stems::resample_to(&input, TARGET_SR, TARGET_SR).expect("resample noop");

    assert_eq!(output.len(), input.len(), "frame count must match");
    let max_dev = output
        .iter()
        .zip(input.iter())
        .map(|(o, i)| (o - i).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        max_dev < 1e-3,
        "passthrough must preserve samples within 1e-3, got {max_dev}"
    );
}

#[test]
fn resample_48k_to_44_1k_produces_expected_frame_count() {
    let input = sine_stereo(48_000, 1, 440.0);
    let output = feature_stems::resample_to(&input, 48_000, TARGET_SR).expect("resample 48k→44.1k");

    let frames = (output.len() / 2) as i32;
    let expected = TARGET_SR as i32;
    let tolerance = expected / 100;
    assert!(
        (frames - expected).abs() <= tolerance,
        "expected ~{expected} ±{tolerance} frames, got {frames}"
    );
    assert_eq!(output.len() % 2, 0, "output must remain stereo interleaved");
}

#[test]
fn resample_returns_input_unchanged_on_empty_buffer() {
    let output = feature_stems::resample_to(&[], 48_000, TARGET_SR).expect("resample empty");
    assert!(output.is_empty());
}

#[test]
fn resample_rejects_odd_length_interleaved_input() {
    let bad = vec![0.0_f32; 1001];
    let err =
        feature_stems::resample_to(&bad, 48_000, TARGET_SR).expect_err("odd length must fail");
    assert!(
        matches!(err, feature_stems::StemError::Resample { .. }),
        "expected Resample error, got {err:?}"
    );
}
