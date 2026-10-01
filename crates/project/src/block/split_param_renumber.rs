//! Responsibility: renumbers a split's knobs once one of its paths is removed.
//!
//! #328 (spec §11.1): removing path `i` drops its knobs and shifts the knobs
//! of every path above it down one, so path `i + 1` keeps its levels, pans
//! and polarity under its new index.

use crate::param::ParameterSet;

use super::split_param_keys::renumbered_key;

/// `params` without the knobs of path `removed`, the knobs above it renamed
/// one index down. Split-wide knobs are kept as they are.
pub fn drop_path_keys(params: &ParameterSet, removed: usize) -> ParameterSet {
    let mut renumbered = ParameterSet::default();
    for (key, value) in &params.values {
        if let Some(new_key) = renumbered_key(key, removed) {
            renumbered.insert(new_key, value.clone());
        }
    }
    renumbered
}
