//! Capture-selection axis filtering for disk-backed NAM/IR plugins.
//!
//! Manifests declare each selectable axis as `parameters[].values`, but two
//! shapes produce dead UI controls — a single-value axis (nothing to choose)
//! and an over-declared axis (a `0..100` knob range where only a handful of
//! values have a backing capture). This module computes the axes worth
//! rendering from the manifest grid, leaving the manifest itself untouched.
//!
//! Issue: #649

use crate::manifest::{Backend, GridCapture, GridParameter, ParameterValue};

/// The declared values of `parameter` that actually appear in at least one
/// `captures[].values` entry, preserving declared order. Values with no
/// backing capture map to no model and are dropped.
fn capture_backed_values(
    parameter: &GridParameter,
    captures: &[GridCapture],
) -> Vec<ParameterValue> {
    parameter
        .values
        .iter()
        .filter(|value| {
            captures
                .iter()
                .any(|capture| capture.values.get(&parameter.name) == Some(*value))
        })
        .cloned()
        .collect()
}

/// One grid axis carrying declared values that no capture references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnbackedAxis {
    pub name: String,
    pub values: Vec<ParameterValue>,
}

/// Declared capture-selection values that no capture references, grouped by
/// axis (declared order). Empty when the grid is fully backed, or for a
/// backend without a capture grid.
///
/// This is the system-level validation read at load time (issue #649): the
/// loader logs a warning naming these values so a malformed manifest is
/// visible, but the package still loads — [`prune_unbacked_grid_values`]
/// cleans the runtime view. It is a diagnostic, never a hard rejection.
pub fn unbacked_grid_values(backend: &Backend) -> Vec<UnbackedAxis> {
    let (parameters, captures) = match backend {
        Backend::Nam {
            parameters,
            captures,
        }
        | Backend::Ir {
            parameters,
            captures,
        } => (parameters, &**captures),
        _ => return Vec::new(),
    };
    parameters
        .iter()
        .filter_map(|parameter| {
            let backed = capture_backed_values(parameter, captures);
            let unbacked: Vec<ParameterValue> = parameter
                .values
                .iter()
                .filter(|value| !backed.contains(*value))
                .cloned()
                .collect();
            (!unbacked.is_empty()).then(|| UnbackedAxis {
                name: parameter.name.clone(),
                values: unbacked,
            })
        })
        .collect()
}

/// Prune every declared grid value that has no backing capture, in place, so
/// a loaded manifest never exposes a selectable value that maps to no model.
///
/// This is the load-time guard (issue #649): applied once when a package is
/// discovered, it makes the in-memory `Backend` the single source of truth
/// for capture-selection axes — every consumer (GUI, MCP, future gRPC) sees
/// only capture-backed values, not just the GUI parameter path. The axis
/// itself is kept even if pruning leaves it with a single value; deciding
/// whether such a dead axis is rendered is [`effective_grid_axes`]'s job.
/// No-op for backends without a capture grid.
pub fn prune_unbacked_grid_values(backend: &mut Backend) {
    let (parameters, captures) = match backend {
        Backend::Nam {
            parameters,
            captures,
        }
        | Backend::Ir {
            parameters,
            captures,
        } => (parameters, &*captures),
        _ => return,
    };
    for parameter in parameters.iter_mut() {
        parameter.values = capture_backed_values(parameter, captures);
    }
}

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
            let backed = capture_backed_values(parameter, captures);
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
