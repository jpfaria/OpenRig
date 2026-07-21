//! Tests for the reference-free tone descriptors.
//!
//! Fixtures are synthetic signals with a known spectral / dynamic character,
//! so every assertion has an unambiguous expected outcome.

use super::*;

const SR: f32 = 48_000.0;

/// One second of a pure sine at `freq` Hz, amplitude `amp`.
fn sine(freq: f32, amp: f32) -> Vec<f32> {
    let n = SR as usize;
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SR).sin())
        .collect()
}

/// Sum of sines at the given (freq, amp) pairs.
fn multitone(components: &[(f32, f32)]) -> Vec<f32> {
    let n = SR as usize;
    (0..n)
        .map(|i| {
            components
                .iter()
                .map(|(f, a)| a * (2.0 * std::f32::consts::PI * f * i as f32 / SR).sin())
                .sum()
        })
        .collect()
}

#[test]
fn clean_1khz_sine_is_ok() {
    let d = analyze_mono(&sine(1_000.0, 0.5), SR);
    assert_eq!(
        d.symptom(),
        Symptom::Ok,
        "a clean 1 kHz sine is healthy: {d:?}"
    );
}

#[test]
fn presence_heavy_signal_reads_as_fizz() {
    // Body note at 1 kHz plus a loud 5 kHz "buzz" on top.
    let d = analyze_mono(&multitone(&[(1_000.0, 0.2), (5_000.0, 0.5)]), SR);
    assert!(
        d.fizz_ratio > FIZZ_RATIO_LIMIT,
        "5 kHz-heavy signal should exceed the fizz limit, got {}",
        d.fizz_ratio
    );
    assert_eq!(d.symptom(), Symptom::Fizz, "{d:?}");
}

#[test]
fn low_mid_heavy_signal_reads_as_mud() {
    // Dominant 300 Hz low-mid with a quiet top.
    let d = analyze_mono(&multitone(&[(300.0, 0.6), (4_000.0, 0.05)]), SR);
    assert!(
        d.mud_ratio > MUD_RATIO_LIMIT,
        "300 Hz-dominant signal should exceed the mud limit, got {}",
        d.mud_ratio
    );
    assert_eq!(d.symptom(), Symptom::Mud, "{d:?}");
}

#[test]
fn clipped_signal_reads_as_clipping() {
    // Overdriven sine clamped hard at the rail.
    let clipped: Vec<f32> = sine(1_000.0, 2.0)
        .iter()
        .map(|s| s.clamp(-1.0, 1.0))
        .collect();
    let d = analyze_mono(&clipped, SR);
    assert!(
        d.clip_fraction > CLIP_FRACTION_LIMIT,
        "hard-clipped sine should register clipping, got {}",
        d.clip_fraction
    );
    assert_eq!(d.symptom(), Symptom::Clipping, "{d:?}");
}

#[test]
fn clipping_wins_over_spectral_tilt() {
    // A signal that is both fizzy and clipped reports clipping first.
    let raw = multitone(&[(1_000.0, 1.5), (5_000.0, 1.5)]);
    let clipped: Vec<f32> = raw.iter().map(|s| s.clamp(-1.0, 1.0)).collect();
    assert_eq!(analyze_mono(&clipped, SR).symptom(), Symptom::Clipping);
}

#[test]
fn silence_is_ok_and_has_no_clipping() {
    let d = analyze_mono(&vec![0.0_f32; SR as usize], SR);
    assert_eq!(d.clip_fraction, 0.0);
    assert_eq!(d.symptom(), Symptom::Ok);
}

#[test]
fn crest_factor_of_sine_is_about_3db() {
    let d = analyze_mono(&sine(1_000.0, 0.5), SR);
    // A sine's crest factor is 20*log10(sqrt(2)) ≈ 3.01 dB.
    assert!(
        (d.crest_db - 3.01).abs() < 0.5,
        "sine crest factor should be ~3 dB, got {}",
        d.crest_db
    );
}

#[test]
fn peak_dbfs_of_half_scale_sine_is_about_minus_6db() {
    let d = analyze_mono(&sine(1_000.0, 0.5), SR);
    assert!(
        (d.peak_dbfs - (-6.02)).abs() < 0.3,
        "0.5-amplitude peak should be ~-6 dBFS, got {}",
        d.peak_dbfs
    );
}

#[test]
fn analyze_stereo_collapses_to_mono() {
    let mono = sine(1_000.0, 0.5);
    let stereo: Vec<[f32; 2]> = mono.iter().map(|&s| [s, s]).collect();
    let dm = analyze_mono(&mono, SR);
    let ds = analyze(&stereo, SR);
    assert!(
        (dm.peak_dbfs - ds.peak_dbfs).abs() < 1e-3,
        "{dm:?} vs {ds:?}"
    );
}

#[test]
fn descriptors_are_deterministic() {
    let sig = multitone(&[(1_000.0, 0.3), (5_000.0, 0.2)]);
    assert_eq!(analyze_mono(&sig, SR), analyze_mono(&sig, SR));
}

#[test]
fn brilliance_heavy_signal_reads_as_harsh() {
    // Body note at 1 kHz plus a loud 11 kHz "ice-pick" in the brilliance band,
    // with no comparable 3-8 kHz presence, so harsh must beat fizz.
    let d = analyze_mono(&multitone(&[(1_000.0, 0.2), (11_000.0, 0.6)]), SR);
    assert_eq!(
        d.symptom(),
        Symptom::Harsh,
        "brilliance-heavy is harsh: {d:?}"
    );
}

#[test]
fn low_end_heavy_signal_reads_as_boomy() {
    // Body note at 1 kHz plus a dominant 60 Hz rumble in the boom band, kept
    // below the rail so it reads as boom, not clipping.
    let d = analyze_mono(&multitone(&[(1_000.0, 0.1), (60.0, 0.5)]), SR);
    assert_eq!(d.symptom(), Symptom::Boomy, "low-end-heavy is boomy: {d:?}");
}

#[test]
fn symptom_with_limits_reclassifies_under_a_stricter_genre() {
    // A mild-presence signal: fizz just under the default 0.05 → healthy by the
    // global bar, but a dark genre (blues-like) sets a tighter fizz limit, so
    // the same tone must read as Fizz for that genre.
    let d = analyze_mono(&multitone(&[(1_000.0, 0.3), (5_000.0, 0.06)]), SR);
    assert_eq!(d.symptom(), Symptom::Ok, "healthy by default bar: {d:?}");

    let strict = SymptomLimits {
        fizz: 0.001,
        ..SymptomLimits::DEFAULT
    };
    assert_eq!(
        d.symptom_with_limits(&strict),
        Symptom::Fizz,
        "the tighter genre limit must flag it: {d:?}"
    );
}

#[test]
fn thin_fires_only_when_the_genre_supplies_a_floor() {
    // A 1 kHz sine has almost no 160–500 Hz low-mid → mud_ratio ~0. That is
    // "thin" only against a genre that expects body; with the deficit floor off
    // (the default) it is not flagged.
    let d = analyze_mono(&sine(1_000.0, 0.5), SR);
    assert!(d.mud_ratio < 0.1, "sine carries little low-mid: {d:?}");
    assert_ne!(
        d.symptom_with_limits(&SymptomLimits::DEFAULT),
        Symptom::Thin,
        "deficit disabled by default: {d:?}"
    );
    let with_body = SymptomLimits {
        thin: 0.2,
        ..SymptomLimits::DEFAULT
    };
    assert_eq!(
        d.symptom_with_limits(&with_body),
        Symptom::Thin,
        "below the genre's low-mid floor → thin: {d:?}"
    );
}

#[test]
fn squash_fires_only_when_the_genre_supplies_a_floor() {
    // A pure sine has a low crest factor (~3 dB) — maximally "squashed"
    // dynamically. Off by default; flagged against a genre that expects
    // transient life.
    let d = analyze_mono(&sine(1_000.0, 0.5), SR);
    assert!(d.crest_db < 6.0, "sine crest is low: {d:?}");
    assert_ne!(
        d.symptom_with_limits(&SymptomLimits::DEFAULT),
        Symptom::Squash,
        "deficit disabled by default: {d:?}"
    );
    let dynamic = SymptomLimits {
        squash: 10.0,
        ..SymptomLimits::DEFAULT
    };
    assert_eq!(
        d.symptom_with_limits(&dynamic),
        Symptom::Squash,
        "below the genre's crest floor → squash: {d:?}"
    );
}

#[test]
fn symptom_with_limits_default_matches_symptom() {
    let d = analyze_mono(&multitone(&[(1_000.0, 0.2), (5_000.0, 0.5)]), SR);
    assert_eq!(d.symptom(), d.symptom_with_limits(&SymptomLimits::DEFAULT));
}

#[test]
fn clean_1khz_sine_has_no_harsh_or_boom() {
    let d = analyze_mono(&sine(1_000.0, 0.5), SR);
    assert!(
        d.harsh_ratio < HARSH_RATIO_LIMIT,
        "clean tone not harsh: {d:?}"
    );
    assert!(
        d.boom_ratio < BOOM_RATIO_LIMIT,
        "clean tone not boomy: {d:?}"
    );
}
