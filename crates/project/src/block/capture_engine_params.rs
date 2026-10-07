//! Responsibility: names the controls a capture plugin's engine adds beside its capture axes.
//!
//! A capture axis named like one of these would collide with the engine's
//! own control in the block, so the plugin editor refuses those names.

/// The IR cab's output level knob.
const IR_OUTPUT: &str = "output_db";

/// Every parameter path the NAM and IR engines add to a capture plugin.
pub fn capture_engine_parameter_names() -> Vec<String> {
    let mut names: Vec<String> = nam::processor::plugin_parameter_specs()
        .into_iter()
        .chain([nam::processor::slim_parameter_spec()])
        .chain(block_reverb::ir_reverb_parameter_specs())
        .map(|spec| spec.path)
        .chain([IR_OUTPUT.to_string()])
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
#[path = "capture_engine_params_tests.rs"]
mod tests;
