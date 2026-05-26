//! Golden roundtrip: decode → resample 44.1k → resample back.
//!
//! Validates that the off-RT pipeline preserves energy and frame count
//! within tolerance for the canonical 48k → 44.1k → 48k path that
//! every Demucs-bound track will travel.

use std::path::Path;

use hound::{SampleFormat, WavSpec, WavWriter};

const SOURCE_SR: u32 = 48_000;
const TARGET_SR: u32 = 44_100;
const DURATION_SECS: u32 = 1;
const FREQ_HZ: f32 = 440.0;

fn write_sine_wav(path: &Path, channels: u16) {
    let spec = WavSpec {
        channels,
        sample_rate: SOURCE_SR,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec).expect("create wav writer");
    let total_frames = (SOURCE_SR * DURATION_SECS) as usize;
    for frame in 0..total_frames {
        let t = frame as f32 / SOURCE_SR as f32;
        let s = (t * FREQ_HZ * std::f32::consts::TAU).sin() * 0.5;
        for _ in 0..channels {
            writer.write_sample(s).expect("write sample");
        }
    }
    writer.finalize().expect("finalize wav");
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

#[test]
fn decode_then_resample_to_target_yields_expected_frame_count_and_rms() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("roundtrip_sine.wav");
    write_sine_wav(&path, 2);

    let decoded = feature_stems::decode_audio(&path).expect("decode source");
    assert_eq!(decoded.sample_rate, SOURCE_SR);

    let original_rms = rms(&decoded.samples);
    let to_target =
        feature_stems::resample_to(&decoded.samples, SOURCE_SR, TARGET_SR).expect("48k→44.1k");

    let target_frames = (to_target.len() / 2) as i32;
    let expected_target = (TARGET_SR * DURATION_SECS) as i32;
    let tolerance_target = expected_target / 100;
    assert!(
        (target_frames - expected_target).abs() <= tolerance_target,
        "target frames {target_frames} outside ±{tolerance_target} of {expected_target}"
    );

    let target_rms = rms(&to_target);
    assert!(
        (target_rms - original_rms).abs() / original_rms < 0.05,
        "RMS drifted >5% across 48k→44.1k: orig={original_rms} target={target_rms}"
    );
}

#[test]
fn full_roundtrip_back_to_source_preserves_rms_within_tolerance() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("roundtrip_sine.wav");
    write_sine_wav(&path, 2);

    let decoded = feature_stems::decode_audio(&path).expect("decode source");
    let original_rms = rms(&decoded.samples);

    let to_target =
        feature_stems::resample_to(&decoded.samples, SOURCE_SR, TARGET_SR).expect("48k→44.1k");
    let back_to_source =
        feature_stems::resample_to(&to_target, TARGET_SR, SOURCE_SR).expect("44.1k→48k");

    let recovered_rms = rms(&back_to_source);
    assert!(
        (recovered_rms - original_rms).abs() / original_rms < 0.05,
        "RMS drifted >5% across full roundtrip: orig={original_rms} recovered={recovered_rms}"
    );

    let recovered_frames = (back_to_source.len() / 2) as i32;
    let expected = (SOURCE_SR * DURATION_SECS) as i32;
    let tolerance = expected / 50;
    assert!(
        (recovered_frames - expected).abs() <= tolerance,
        "recovered frames {recovered_frames} outside ±{tolerance} of {expected}"
    );
}
