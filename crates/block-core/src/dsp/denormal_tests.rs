
use super::*;

#[test]
fn guard_is_above_subnormal_threshold() {
    // f32::MIN_POSITIVE is the smallest *normal* positive — anything
    // below it is subnormal.
    assert!(DENORMAL_GUARD > f32::MIN_POSITIVE);
}

#[test]
fn guard_is_far_below_audio_noise_floor() {
    // -96 dBFS ≈ 1.5e-5 — even 24-bit signal shouldn't notice.
    assert!(DENORMAL_GUARD < 1.0e-10);
}

#[test]
fn flush_pushes_subnormal_back_to_normal() {
    let subnormal = 1.0e-40_f32; // subnormal in f32
    let flushed = flush_denormal(subnormal);
    assert!(flushed >= f32::MIN_POSITIVE, "still subnormal: {flushed}");
}

#[test]
fn flush_is_audio_transparent_for_normal_input() {
    let signal = 0.5_f32;
    let flushed = flush_denormal(signal);
    assert!((flushed - signal).abs() < 1.0e-20);
}
