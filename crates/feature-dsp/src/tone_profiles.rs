//! Genre-calibrated Tone Doctor limits (#809, Piece 1 — aggregation core).
//!
//! Pure, I/O-free aggregation: in = a set of genre-labeled [`ToneDescriptors`]
//! measured from real isolated-guitar stems, out = one [`GenreProfile`] per
//! genre whose `mud` / `fizz` / `clip` limits are the healthy distribution's
//! high percentile. This is what turns the provisional global constants in
//! [`crate::tone_descriptors`] into measured, per-genre numbers.
//!
//! The binary that walks the stems, runs [`crate::tone_descriptors::analyze`],
//! and writes the table owns all the I/O; this module owns only the maths, so
//! it is deterministic (invariant #9) and unit-testable without files.

use crate::tone_descriptors::ToneDescriptors;
use std::collections::BTreeMap;

/// Default healthy-distribution percentile a limit is read off at.
pub const DEFAULT_PERCENTILE: f32 = 0.90;
/// Fewer contributing stems than this and a genre's profile is `Provisional`.
pub const MIN_CONFIDENT_SAMPLES: usize = 6;

/// How much to trust a genre's calibrated limits, given how many stems fed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// At least [`MIN_CONFIDENT_SAMPLES`] stems — the distribution is solid.
    Trusted,
    /// Too few stems — the limit is a best guess, surface it as such.
    Provisional,
}

/// One genre's calibrated symptom limits plus the evidence behind them.
#[derive(Debug, Clone, PartialEq)]
pub struct GenreProfile {
    pub genre: String,
    pub mud_limit: f32,
    pub fizz_limit: f32,
    pub clip_limit: f32,
    pub boom_limit: f32,
    /// Deficit floor: `mud_ratio` below it reads as `Thin` (the low percentile
    /// of the genre's low-mid — less body than the style expects).
    pub thin_limit: f32,
    /// Deficit floor: `crest_db` below it reads as `Squash` (the low percentile
    /// of the genre's crest factor — flatter dynamics than the style expects).
    pub squash_limit: f32,
    /// Stems that contributed to this genre.
    pub n: usize,
    pub confidence: Confidence,
}

/// Calibrate per-genre limits from genre-labeled descriptor samples.
///
/// `percentile` is a fraction in `0.0..=1.0` (see [`DEFAULT_PERCENTILE`]).
/// Returns one profile per distinct genre, sorted by genre name (the `BTreeMap`
/// keeps that ordering deterministic — invariant #9).
pub fn calibrate(samples: &[(String, ToneDescriptors)], percentile: f32) -> Vec<GenreProfile> {
    // Collect each metric's values per genre.
    let mut by_genre: BTreeMap<&str, MetricColumns> = BTreeMap::new();
    for (genre, d) in samples {
        let cols = by_genre.entry(genre.as_str()).or_default();
        cols.mud.push(d.mud_ratio);
        cols.fizz.push(d.fizz_ratio);
        cols.clip.push(d.clip_fraction);
        cols.boom.push(d.boom_ratio);
        cols.crest.push(d.crest_db);
    }

    // Deficit floors read the *low* tail (e.g. p10 when percentile is p90).
    let low = 1.0 - percentile;

    by_genre
        .into_iter()
        .map(
            |(
                genre,
                MetricColumns {
                    mud,
                    fizz,
                    clip,
                    boom,
                    crest,
                },
            )| {
                let n = mud.len();
                GenreProfile {
                    genre: genre.to_string(),
                    mud_limit: percentile_of(mud.clone(), percentile),
                    fizz_limit: percentile_of(fizz, percentile),
                    clip_limit: percentile_of(clip, percentile),
                    boom_limit: percentile_of(boom, percentile),
                    thin_limit: percentile_of(mud, low),
                    squash_limit: percentile_of(crest, low),
                    n,
                    confidence: if n >= MIN_CONFIDENT_SAMPLES {
                        Confidence::Trusted
                    } else {
                        Confidence::Provisional
                    },
                }
            },
        )
        .collect()
}

/// One genre's raw metric values, accumulated before the percentile is taken.
#[derive(Default)]
struct MetricColumns {
    mud: Vec<f32>,
    fizz: Vec<f32>,
    clip: Vec<f32>,
    boom: Vec<f32>,
    crest: Vec<f32>,
}

/// The `percentile` (0.0..=1.0) of `values`, by linear interpolation between the
/// two nearest ranks. A single value returns itself; empty returns 0.0.
fn percentile_of(mut values: Vec<f32>, percentile: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if values.len() == 1 {
        return values[0];
    }
    let rank = percentile.clamp(0.0, 1.0) * (values.len() - 1) as f32;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    let frac = rank - lower as f32;
    values[lower] + frac * (values[upper] - values[lower])
}

#[cfg(test)]
#[path = "tone_profiles_tests.rs"]
mod tests;
