//! NAM (`backend: nam`) manifest → parameter schema.
//!
//! A NAM block has two layers of controls with distinct, always-known
//! origins, so the block editor can split them into tabs with no
//! per-plugin authoring (issue #786):
//!
//! - **Capture** — the manifest `parameters:` axes. They pick which
//!   `.nam` capture is loaded (channel, gain, mic, …), and they are the
//!   only thing a manifest declares.
//! - the engine defaults every NAM block has regardless of the capture —
//!   input / output makeup (+ the A2 slim lever), the noise gate and the EQ.
//!   They carry their own tabs (`Amp` / `Noise Gate` / `EQ`), declared once
//!   with the specs in `nam::params` since they are the same for every NAM.

use super::grid_schema::grid_parameter_to_spec;

/// Editor tab holding the manifest capture-selection axes.
pub(crate) const NAM_CAPTURE_GROUP: &str = "Capture";

pub(crate) fn nam_parameters(
    package: &plugin_loader::LoadedPackage,
    parameters: &[plugin_loader::manifest::GridParameter],
    captures: &[plugin_loader::manifest::GridCapture],
) -> Vec<block_core::param::ParameterSpec> {
    // Pre-#287 (when NAM amps lived in `block-preamp/src/nam_*.rs`),
    // every NAM model exposed two layers of knobs: the per-capture
    // grid (e.g. `mode`, `character` for nam_boss_ds_2) AND the 8
    // universal NAM plugin knobs (input/output level, noise gate,
    // EQ on/off + bass/mid/treble) added by `nam::plugin_parameter_specs()`.
    // The migration to disk packages dropped the second layer, so
    // every NAM in the GUI lost its standard knobs (~96 packages —
    // 21 with empty grids ended up with zero knobs at all). Merge
    // the standard set back in. Issue #401.
    //
    // `effective_grid_axes` first drops dead capture-selector axes
    // (single-value or over-declared dropdowns) — issue #649. A NAM whose
    // axes are all dead ends up with the Amp group alone, so the editor
    // renders no tab bar rather than an empty Capture tab.
    let axes = plugin_loader::grid_axes::effective_grid_axes(parameters, captures);
    let mut specs: Vec<block_core::param::ParameterSpec> = axes
        .iter()
        .map(|axis| grid_parameter_to_spec(axis, Some(NAM_CAPTURE_GROUP)))
        .collect();
    // Issue #496 reverses #402's "drop output_db". With the audit-side
    // `output_gain_db` cleared in the manifests, there was no automatic
    // compensation AND no user-facing knob — every NAM played at the raw
    // (quiet) capture output. Re-expose the host's Output knob so the user
    // can add makeup gain; the manifest `output_gain_db` is still summed on
    // top when present.
    specs.extend(nam::processor::plugin_parameter_specs());
    // Issue #657: NAM/A2 (SlimmableContainer) models expose a runtime size
    // lever (SetSlimmableSize). A1 models are not slimmable, so the knob is
    // appended only for A2 — driven by the manifest's declared architecture
    // (issue #650).
    if package.manifest.architecture == Some(plugin_loader::manifest::NamArchitecture::A2) {
        specs.push(nam::processor::slim_parameter_spec());
    }
    specs
}

#[cfg(test)]
#[path = "nam_schema_tests.rs"]
mod tests;
