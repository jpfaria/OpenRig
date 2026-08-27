//! Responsibility: picks the capture that matches the parameters the user set.

use crate::manifest::{GridCapture, GridParameter, ParameterValue};
use block_core::param::ParameterSet;

/// Pick the [`GridCapture`] whose declared values are closest to the
/// user's `params` for every declared `parameters` axis.
///
/// For numeric axes the user value is snapped to the nearest declared
/// value (linear distance). For text axes an exact match is required;
/// when the user's value is missing or doesn't match, the first capture
/// that matches the other axes wins.
///
/// Returns `None` if `captures` is empty or no capture matches all
/// declared text axes.
/// Seed the born-default grid-axis values for a freshly created block from
/// the FIRST declared capture (issue #630).
///
/// A grid pedal must NOT be born at the per-axis-minimum combination: that is
/// computed by defaulting each axis independently to its first declared value
/// and, for a multi-axis grid, can produce a cell that does NOT exist in the
/// capture list (and historically defaulted to `drive=0`/`level=0`, which the
/// removed #402 rule then treated as "off"). The manifest lists its captures
/// in order, so the first capture is a deterministic, REAL grid point — the
/// block is born there and is audible immediately.
///
/// Returns the `(axis_name, ParameterValue)` pairs of the first capture,
/// restricted to the declared `parameters` axes (capture-only keys that aren't
/// real axes are ignored). Returns an empty vec when there are no captures or
/// no declared axes — callers then fall back to the per-axis spec defaults.
pub fn first_capture_axis_values(
    parameters: &[GridParameter],
    captures: &[GridCapture],
) -> Vec<(String, ParameterValue)> {
    let Some(first) = captures.first() else {
        return Vec::new();
    };
    parameters
        .iter()
        .filter_map(|parameter| {
            first
                .values
                .get(&parameter.name)
                .map(|value| (parameter.name.clone(), value.clone()))
        })
        .collect()
}

pub fn resolve_capture<'a>(
    parameters: &[GridParameter],
    captures: &'a [GridCapture],
    params: &ParameterSet,
) -> Option<&'a GridCapture> {
    if captures.is_empty() {
        return None;
    }
    if parameters.is_empty() {
        return captures.first();
    }
    let snapped: Vec<(String, ParameterValue)> = parameters
        .iter()
        .map(|parameter| {
            let value = snap_user_value(parameter, params);
            (parameter.name.clone(), value)
        })
        .collect();
    captures
        .iter()
        .min_by(|left, right| score(left, &snapped).cmp(&score(right, &snapped)))
}

/// Snap the user's value for `parameter` to the nearest declared value
/// (numeric) or pick the first declared (text fallback).
fn snap_user_value(parameter: &GridParameter, params: &ParameterSet) -> ParameterValue {
    let user_text = params.get_string(&parameter.name);
    if let Some(text) = user_text {
        for declared in &parameter.values {
            if let ParameterValue::Text(declared_text) = declared {
                if declared_text == text {
                    return ParameterValue::Text(text.to_string());
                }
            }
        }
    }
    if let Some(user_number) = params.get_f32(&parameter.name) {
        let mut best = parameter
            .values
            .first()
            .cloned()
            .unwrap_or(ParameterValue::Number(0.0));
        let mut best_dist = f64::INFINITY;
        for declared in &parameter.values {
            if let ParameterValue::Number(declared_value) = declared {
                let dist = ((*declared_value) - f64::from(user_number)).abs();
                if dist < best_dist {
                    best_dist = dist;
                    best = ParameterValue::Number(*declared_value);
                }
            }
        }
        return best;
    }
    parameter
        .values
        .first()
        .cloned()
        .unwrap_or(ParameterValue::Number(0.0))
}

/// Lower is better. Sums per-axis mismatches.
fn score(capture: &GridCapture, snapped: &[(String, ParameterValue)]) -> u64 {
    let mut total: u64 = 0;
    for (name, target) in snapped {
        match capture.values.get(name) {
            Some(actual) if values_equal(actual, target) => {}
            _ => total = total.saturating_add(1),
        }
    }
    total
}

fn values_equal(left: &ParameterValue, right: &ParameterValue) -> bool {
    match (left, right) {
        (ParameterValue::Number(a), ParameterValue::Number(b)) => a.to_bits() == b.to_bits(),
        (ParameterValue::Text(a), ParameterValue::Text(b)) => a == b,
        _ => false,
    }
}
