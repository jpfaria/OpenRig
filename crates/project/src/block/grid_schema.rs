//! Capture-grid axis → `ParameterSpec`. Shared by the NAM and IR
//! backends: both describe their captures as a grid of axes
//! (`manifest.parameters`) and both turn one axis into one control.

use super::manifest_labels::sanitize_label;

/// One manifest grid axis becomes one control: numeric values → float
/// knob spanning the declared min..max, bool values → toggle, anything
/// else → enum dropdown. `group` is the editor tab the control lands in
/// (`None` = the block's single flat grid).
pub(crate) fn grid_parameter_to_spec(
    parameter: &plugin_loader::manifest::GridParameter,
    group: Option<&str>,
) -> block_core::param::ParameterSpec {
    use plugin_loader::manifest::ParameterValue;
    // Sanitise the axis name so emojis baked into third-party manifests
    // (issue #424 — Bogner Ecstasy) don't tofu in the BlockEditorPanel
    // header; raw `parameter.name` stays untouched as the lookup key.
    let raw_label = parameter.display_name.as_deref().unwrap_or(&parameter.name);
    let label = sanitize_label(raw_label);
    let all_numeric = parameter
        .values
        .iter()
        .all(|v| matches!(v, ParameterValue::Number(_)));
    if all_numeric && !parameter.values.is_empty() {
        let numbers: Vec<f64> = parameter
            .values
            .iter()
            .filter_map(|v| match v {
                ParameterValue::Number(n) => Some(*n),
                _ => None,
            })
            .collect();
        let min = numbers.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = numbers.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let default = numbers.first().copied().unwrap_or(min);
        let step = if numbers.len() > 1 {
            (numbers[1] - numbers[0]).abs() as f32
        } else {
            1.0_f32
        };
        block_core::param::float_parameter(
            &parameter.name,
            &label,
            group,
            Some(default as f32),
            min as f32,
            max as f32,
            step.max(0.01),
            block_core::param::ParameterUnit::None,
        )
    } else if parameter
        .values
        .iter()
        .all(|v| matches!(v, ParameterValue::Bool(_)))
        && !parameter.values.is_empty()
    {
        // Pure bool grid → render as a toggle. The default mirrors the
        // first listed value so manifests can pick the natural off-state
        // (`[false, true]` -> default off).
        let default = parameter.values.iter().find_map(|v| match v {
            ParameterValue::Bool(b) => Some(*b),
            _ => None,
        });
        block_core::param::bool_parameter(&parameter.name, &label, group, default)
    } else {
        // (raw_value, sanitised_label) pairs — the value is the lookup
        // key into `captures[].values` and the user's persisted
        // `ParameterSet`, so it must round-trip byte-for-byte. Only the
        // displayed label is cleaned of emoji (issue #424).
        let options: Vec<(String, String)> = parameter
            .values
            .iter()
            .map(|value| {
                let raw = match value {
                    ParameterValue::Text(t) => t.clone(),
                    ParameterValue::Number(n) => n.to_string(),
                    ParameterValue::Bool(b) => b.to_string(),
                };
                let display = sanitize_label(&raw);
                (raw, display)
            })
            .collect();
        let option_refs: Vec<(&str, &str)> = options
            .iter()
            .map(|(value, display)| (value.as_str(), display.as_str()))
            .collect();
        let default = options.first().map(|(k, _)| k.as_str());
        block_core::param::enum_parameter(&parameter.name, &label, group, default, &option_refs)
    }
}
