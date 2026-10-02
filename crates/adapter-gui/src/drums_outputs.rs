//! Responsibility: lists the endpoints the drum machine can play to.

use crate::metronome_view::{output_endpoints, MetronomeOutput};

/// The output endpoints of the project's output-only bindings: an in+out
/// binding repeats an output another binding already names. With no
/// output-only binding (a fresh install), every output, so the drums sound.
pub(crate) fn drums_output_endpoints(
    bindings: &[infra_filesystem::IoBinding],
) -> Vec<MetronomeOutput> {
    let output_only: Vec<infra_filesystem::IoBinding> = bindings
        .iter()
        .filter(|b| b.inputs.is_empty())
        .cloned()
        .collect();
    if output_only.is_empty() {
        output_endpoints(bindings)
    } else {
        output_endpoints(&output_only)
    }
}

#[cfg(test)]
#[path = "drums_outputs_tests.rs"]
mod tests;
