//! Capture-selection axis filtering for disk-backed NAM/IR plugins.
//!
//! Manifests declare each selectable axis as `parameters[].values`, but two
//! shapes produce dead UI controls — a single-value axis (nothing to choose)
//! and an over-declared axis (a `0..100` knob range where only a handful of
//! values have a backing capture). This module computes the axes worth
//! rendering from the manifest grid, leaving the manifest itself untouched.
//!
//! Issue: #649

use crate::manifest::{GridCapture, GridParameter, ParameterValue};

/// Restrict the declared grid axes to the values actually backed by a
/// capture, dropping axes that end up with fewer than two backed values.
///
/// For each axis this keeps only the declared values that appear in at least
/// one `captures[].values` entry, preserving declared order, then discards
/// the whole axis when fewer than two backed values remain — its single
/// capture still loads via `dispatch::resolve_capture` (a dropped axis holds
/// the same value across every capture, so it never changes the score). The
/// result is purely the UI/parameter-generation view; the manifest is the
/// untouched source of truth.
pub fn effective_grid_axes(
    parameters: &[GridParameter],
    captures: &[GridCapture],
) -> Vec<GridParameter> {
    parameters
        .iter()
        .filter_map(|parameter| {
            let backed: Vec<ParameterValue> = parameter
                .values
                .iter()
                .filter(|value| {
                    captures
                        .iter()
                        .any(|capture| capture.values.get(&parameter.name) == Some(*value))
                })
                .cloned()
                .collect();
            (backed.len() >= 2).then(|| GridParameter {
                name: parameter.name.clone(),
                display_name: parameter.display_name.clone(),
                values: backed,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "grid_axes_tests.rs"]
mod tests;
