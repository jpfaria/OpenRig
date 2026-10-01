//! Responsibility: names the per-path knob keys of a split.
//!
//! #328 (spec §11.2): every path `i` of a split has its own knobs, keyed with
//! an `_<i>` suffix (`level_to_0`, `mix_pan_2`, …). Files saved before §11 used
//! `_a`/`_b` suffixes and `mix_b_polarity`; they load as paths 0 and 1.

use crate::param::ParameterSet;

pub const LEVEL_TO: &str = "level_to_";
pub const BALANCE: &str = "balance_";
pub const MIX_LEVEL: &str = "mix_level_";
pub const MIX_PAN: &str = "mix_pan_";
pub const MIX_POLARITY: &str = "mix_polarity_";

/// Every per-path stem, in the order the schema lists them.
pub const PATH_STEMS: [&str; 5] = [LEVEL_TO, BALANCE, MIX_LEVEL, MIX_PAN, MIX_POLARITY];

pub fn level_to(path: usize) -> String {
    format!("{LEVEL_TO}{path}")
}

pub fn balance(path: usize) -> String {
    format!("{BALANCE}{path}")
}

pub fn mix_level(path: usize) -> String {
    format!("{MIX_LEVEL}{path}")
}

pub fn mix_pan(path: usize) -> String {
    format!("{MIX_PAN}{path}")
}

pub fn mix_polarity(path: usize) -> String {
    format!("{MIX_POLARITY}{path}")
}

/// The stem and path index of a per-path key, or `None` for a split-wide key.
pub fn path_of_key(key: &str) -> Option<(&'static str, usize)> {
    PATH_STEMS.iter().find_map(|stem| {
        key.strip_prefix(stem)
            .and_then(|index| index.parse().ok())
            .map(|index| (*stem, index))
    })
}

/// The key of `key` once path `removed` is gone: `None` when it belongs to
/// that path, the key shifted down one when its path sits above it, the same
/// key otherwise.
pub fn renumbered_key(key: &str, removed: usize) -> Option<String> {
    match path_of_key(key) {
        Some((_, path)) if path == removed => None,
        Some((stem, path)) if path > removed => Some(format!("{stem}{}", path - 1)),
        _ => Some(key.to_string()),
    }
}

/// Rename the pre-§11 `_a`/`_b` keys to paths 0 and 1. A key already in the
/// new form wins over its legacy twin.
pub fn migrate_legacy_split_keys(params: ParameterSet) -> ParameterSet {
    let mut migrated = ParameterSet::default();
    for (key, value) in &params.values {
        if let Some(new_key) = legacy_key(key) {
            migrated.insert(new_key, value.clone());
        }
    }
    for (key, value) in params.values {
        if legacy_key(&key).is_none() {
            migrated.insert(key, value);
        }
    }
    migrated
}

fn legacy_key(key: &str) -> Option<String> {
    if key == "mix_b_polarity" {
        return Some(mix_polarity(1));
    }
    PATH_STEMS
        .iter()
        .find_map(|stem| match key.strip_prefix(stem) {
            Some("a") => Some(format!("{stem}0")),
            Some("b") => Some(format!("{stem}1")),
            _ => None,
        })
}
