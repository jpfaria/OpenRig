//! Tests for the embedded genre-limit table.

use super::*;
use feature_dsp::tone_descriptors::FIZZ_RATIO_LIMIT;

#[test]
fn embedded_table_resolves_a_known_genre_to_its_calibrated_limits() {
    let table = ProfileTable::embedded();
    // grunge is a trusted genre in the shipped table with a high fizz limit —
    // very different from the global default, which is the whole point.
    let grunge = table.limits_for(Some("grunge"));
    assert!(
        grunge.fizz > 0.3,
        "grunge's calibrated fizz limit is high (buzzy is its signature), got {}",
        grunge.fizz
    );
    assert!(
        (grunge.fizz - FIZZ_RATIO_LIMIT).abs() > 0.1,
        "and it differs from the global default {FIZZ_RATIO_LIMIT}"
    );
}

#[test]
fn unknown_or_absent_genre_falls_back_to_defaults() {
    let table = ProfileTable::embedded();
    assert_eq!(table.limits_for(None), SymptomLimits::DEFAULT);
    assert_eq!(table.limits_for(Some("no-such-genre")), SymptomLimits::DEFAULT);
}

#[test]
fn table_lists_its_genres() {
    let table = ProfileTable::embedded();
    let genres = table.genres();
    assert!(genres.contains(&"grunge"), "genres: {genres:?}");
    assert!(genres.contains(&"blues-rock"), "genres: {genres:?}");
}
