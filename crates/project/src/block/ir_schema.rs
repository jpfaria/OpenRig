//! IR (`backend: ir`) manifest → parameter schema.

use super::grid_schema::grid_parameter_to_spec;

pub(crate) fn ir_parameters(
    package: &plugin_loader::LoadedPackage,
    parameters: &[plugin_loader::manifest::GridParameter],
    captures: &[plugin_loader::manifest::GridCapture],
) -> Vec<block_core::param::ParameterSpec> {
    // Same dead-axis filter as NAM (issue #649). IR params stay ungrouped:
    // the block editor renders them as one flat grid.
    let axes = plugin_loader::grid_axes::effective_grid_axes(parameters, captures);
    let mut specs: Vec<block_core::param::ParameterSpec> = axes
        .iter()
        .map(|axis| grid_parameter_to_spec(axis, None))
        .collect();
    // Issue #733: a `type: reverb` IR blends dry/wet rather than
    // playing 100% wet at a calibrated level, so it exposes the
    // reverb controls (mix / pre-delay / wet level) in place of the
    // cab-style absolute Output knob.
    if package.manifest.block_type == plugin_loader::manifest::BlockType::Reverb {
        specs.extend(block_reverb::ir_reverb_parameter_specs());
        return specs;
    }
    // Issue #655: user-adjustable Output Level knob (mirrors NAM).
    // The default mirrors the engine baseline — the first capture's
    // audit (manifest-level fallback, 0 dB if neither) — so the knob
    // shows the real applied offset and a fresh block born at the
    // first capture stays unchanged (volume invariant #10). The
    // audio path resolves the offset per-capture from the raw saved
    // params (see `ir::from_package::resolve_output_db`); this
    // default only drives the UI and the new-block seed.
    let default_db = captures
        .first()
        .and_then(|c| c.output_gain_db)
        .or(package.manifest.output_gain_db)
        .unwrap_or(0.0);
    specs.push(block_core::param::float_parameter(
        "output_db",
        "Output",
        None,
        Some(default_db),
        -24.0,
        24.0,
        0.1,
        block_core::param::ParameterUnit::Decibels,
    ));
    specs
}

#[cfg(test)]
#[path = "ir_schema_tests.rs"]
mod tests;
