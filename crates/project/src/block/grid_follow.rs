//! Responsibility: moves a block's capture values from one capture grid to another.
//!
//! A capture file never moves when a plugin's parameters are edited; only
//! the values that point at it change. A block keeps playing the same
//! capture by finding it in the old grid and taking its values in the new
//! one.

use domain::value_objects::ParameterValue;
use plugin_loader::grid_axes::effective_grid_axes;
use plugin_loader::manifest::{GridCapture, GridParameter};

use crate::param::ParameterSet;

/// The capture whose values `params` names exactly on every axis the block
/// can set, or `None` when the values belong to no capture of this grid.
pub fn exact_capture<'a>(
    parameters: &[GridParameter],
    captures: &'a [GridCapture],
    params: &ParameterSet,
) -> Option<&'a GridCapture> {
    let axes = effective_grid_axes(parameters, captures);
    captures.iter().find(|capture| {
        axes.iter().all(|axis| {
            let (Some(stored), Some(value)) =
                (params.get(&axis.name), capture.values.get(&axis.name))
            else {
                return false;
            };
            same_value(stored, &axis_param_value(axis, value))
        })
    })
}

/// `params` with the capture they select in the `from` grid expressed in
/// the `to` grid, or `None` when that capture's file is not in `to`.
pub fn follow_capture(
    from_parameters: &[GridParameter],
    from_captures: &[GridCapture],
    to_parameters: &[GridParameter],
    to_captures: &[GridCapture],
    params: &ParameterSet,
) -> Option<ParameterSet> {
    let from = exact_capture(from_parameters, from_captures, params)?;
    let to = to_captures.iter().find(|c| c.file == from.file)?;
    let mut moved = params.clone();
    for parameter in from_parameters {
        moved.values.remove(&parameter.name);
    }
    for parameter in to_parameters {
        if let Some(value) = to.values.get(&parameter.name) {
            moved.insert(&parameter.name, axis_param_value(parameter, value));
        }
    }
    Some(moved)
}

/// The value a block stores for `value` on `parameter`: a number on a knob,
/// a boolean on a switch, the option text on a choice.
pub fn axis_param_value(
    parameter: &GridParameter,
    value: &plugin_loader::manifest::ParameterValue,
) -> ParameterValue {
    use plugin_loader::manifest::ParameterValue as M;
    let all =
        |test: fn(&M) -> bool| !parameter.values.is_empty() && parameter.values.iter().all(test);
    match value {
        M::Number(n) if all(|v| matches!(v, M::Number(_))) => ParameterValue::Float(*n as f32),
        M::Bool(b) if all(|v| matches!(v, M::Bool(_))) => ParameterValue::Bool(*b),
        M::Text(t) => ParameterValue::String(t.clone()),
        M::Number(n) => ParameterValue::String(n.to_string()),
        M::Bool(b) => ParameterValue::String(b.to_string()),
    }
}

fn same_value(stored: &ParameterValue, wanted: &ParameterValue) -> bool {
    match (number(stored), number(wanted)) {
        (Some(a), Some(b)) => (a - b).abs() < 1e-6,
        _ => stored == wanted,
    }
}

fn number(value: &ParameterValue) -> Option<f64> {
    match value {
        ParameterValue::Float(f) => Some(f64::from(*f)),
        ParameterValue::Int(i) => Some(*i as f64),
        _ => None,
    }
}

#[cfg(test)]
#[path = "grid_follow_tests.rs"]
mod tests;
