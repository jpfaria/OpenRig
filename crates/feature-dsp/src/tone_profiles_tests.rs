//! Tests for the genre-calibration aggregation core.
//!
//! Every fixture is a set of synthetic descriptors with a known distribution,
//! so each calibrated limit has an analytically checkable expected value.

use super::*;

/// A descriptor carrying only the three fields calibration reads; the rest are
/// irrelevant here and left at neutral values.
fn desc(mud: f32, fizz: f32, clip: f32) -> ToneDescriptors {
    ToneDescriptors {
        rms_dbfs: -20.0,
        peak_dbfs: -6.0,
        crest_db: 14.0,
        clip_fraction: clip,
        fizz_ratio: fizz,
        mud_ratio: mud,
    }
}

/// `n` identical samples of a genre, so the percentile is unambiguous.
fn repeat(genre: &str, d: ToneDescriptors, n: usize) -> Vec<(String, ToneDescriptors)> {
    (0..n).map(|_| (genre.to_string(), d)).collect()
}

#[test]
fn groups_by_genre_one_profile_each_sorted() {
    let mut samples = repeat("metal", desc(0.6, 0.2, 0.01), 3);
    samples.extend(repeat("blues", desc(0.3, 0.03, 0.0), 3));
    samples.extend(repeat("clean", desc(0.2, 0.01, 0.0), 3));

    let profiles = calibrate(&samples, DEFAULT_PERCENTILE);

    let genres: Vec<&str> = profiles.iter().map(|p| p.genre.as_str()).collect();
    assert_eq!(
        genres,
        vec!["blues", "clean", "metal"],
        "one profile per genre, sorted by name: {profiles:?}"
    );
    assert!(
        profiles.iter().all(|p| p.n == 3),
        "each genre saw 3 stems: {profiles:?}"
    );
}

#[test]
fn limit_is_the_requested_percentile_of_the_distribution() {
    // mud_ratio ascends 0.10, 0.20, ..., 1.00 (10 values). p90 with linear
    // interpolation sits at rank 0.9*(10-1)=8.1 → between the 9th (0.90) and
    // 10th (1.00) values: 0.90 + 0.1*(1.00-0.90) = 0.91.
    let samples: Vec<(String, ToneDescriptors)> = (1..=10)
        .map(|i| ("rock".to_string(), desc(i as f32 / 10.0, 0.0, 0.0)))
        .collect();

    let profiles = calibrate(&samples, 0.90);
    assert_eq!(profiles.len(), 1);
    assert!(
        (profiles[0].mud_limit - 0.91).abs() < 1e-4,
        "p90 of 0.1..1.0 is 0.91, got {}",
        profiles[0].mud_limit
    );
}

#[test]
fn few_samples_are_provisional_enough_are_trusted() {
    let thin = repeat("fuzz", desc(0.5, 0.1, 0.0), MIN_CONFIDENT_SAMPLES - 1);
    let thick = repeat("hard-rock", desc(0.5, 0.1, 0.0), MIN_CONFIDENT_SAMPLES);
    let mut samples = thin;
    samples.extend(thick);

    let profiles = calibrate(&samples, DEFAULT_PERCENTILE);
    let fuzz = profiles.iter().find(|p| p.genre == "fuzz").unwrap();
    let hard = profiles.iter().find(|p| p.genre == "hard-rock").unwrap();

    assert_eq!(fuzz.confidence, Confidence::Provisional, "{fuzz:?}");
    assert_eq!(hard.confidence, Confidence::Trusted, "{hard:?}");
}

#[test]
fn single_sample_returns_that_value_and_is_provisional() {
    let samples = repeat("jazz", desc(0.42, 0.07, 0.002), 1);
    let profiles = calibrate(&samples, DEFAULT_PERCENTILE);

    assert_eq!(profiles.len(), 1);
    let p = &profiles[0];
    assert!((p.mud_limit - 0.42).abs() < 1e-6, "{p:?}");
    assert!((p.fizz_limit - 0.07).abs() < 1e-6, "{p:?}");
    assert!((p.clip_limit - 0.002).abs() < 1e-6, "{p:?}");
    assert_eq!(p.confidence, Confidence::Provisional);
    assert_eq!(p.n, 1);
}

#[test]
fn calibration_is_deterministic() {
    let mut samples = repeat("metal", desc(0.6, 0.2, 0.01), 4);
    samples.extend(repeat("blues", desc(0.3, 0.03, 0.0), 7));

    let a = calibrate(&samples, DEFAULT_PERCENTILE);
    let b = calibrate(&samples, DEFAULT_PERCENTILE);
    assert_eq!(a, b, "same input must yield identical profiles");
}
