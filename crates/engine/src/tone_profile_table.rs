//! Runtime access to the genre-calibrated Tone Doctor limits (#809, Piece 2).
//!
//! The offline calibrator writes `assets/tone-profiles/profiles.yaml`; that file
//! is embedded at build time and parsed once into a `genre -> SymptomLimits`
//! table. The live doctor resolves the player's selected genre through
//! [`ProfileTable::limits_for`] and feeds the result to
//! [`crate::tone_doctor::diagnose_with_limits`] — so the same tone is judged by
//! its style's calibrated bar instead of the global defaults.

use feature_dsp::tone_descriptors::SymptomLimits;
use std::collections::BTreeMap;

/// The calibrated table, baked into the binary from the versioned asset.
const EMBEDDED: &str = include_str!("../../../assets/tone-profiles/profiles.yaml");

/// One genre row as it appears in `profiles.yaml` (n/confidence ignored here).
#[derive(serde::Deserialize)]
struct Row {
    mud: f32,
    fizz: f32,
    clip: f32,
    harsh: f32,
    boom: f32,
    thin: f32,
    squash: f32,
}

/// Genre → calibrated limits, sorted by genre name.
pub struct ProfileTable {
    by_genre: BTreeMap<String, SymptomLimits>,
}

impl ProfileTable {
    /// The table baked in from `assets/tone-profiles/profiles.yaml`.
    pub fn embedded() -> Self {
        parse(EMBEDDED)
    }

    /// Limits for `genre` (as chosen by the player), or [`SymptomLimits::DEFAULT`]
    /// when no genre is selected or the genre is unknown to the table.
    pub fn limits_for(&self, genre: Option<&str>) -> SymptomLimits {
        genre
            .and_then(|g| self.by_genre.get(g))
            .copied()
            .unwrap_or(SymptomLimits::DEFAULT)
    }

    /// The genres the table knows, sorted — for the UI selector.
    pub fn genres(&self) -> Vec<&str> {
        self.by_genre.keys().map(String::as_str).collect()
    }
}

/// Parse a `profiles.yaml` string into a table.
fn parse(raw: &str) -> ProfileTable {
    let rows: BTreeMap<String, Row> = serde_yaml::from_str(raw).unwrap_or_default();
    let by_genre = rows
        .into_iter()
        .map(|(genre, r)| {
            (
                genre,
                SymptomLimits {
                    mud: r.mud,
                    fizz: r.fizz,
                    harsh: r.harsh,
                    boom: r.boom,
                    clip: r.clip,
                    thin: r.thin,
                    squash: r.squash,
                },
            )
        })
        .collect();
    ProfileTable { by_genre }
}

#[cfg(test)]
#[path = "tone_profile_table_tests.rs"]
mod tests;
