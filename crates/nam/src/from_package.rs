//! Generic NAM instantiation from a `plugin_loader::LoadedPackage`.
//!
//! Picks the capture file that matches the user's `ParameterSet` (axes
//! declared in the manifest) and hands it to the existing
//! [`crate::build_processor_with_assets_for_layout`].
//!
//! Issues: #287 (loader), #630 (grid knobs always select the nearest
//! capture — see below).

use anyhow::{anyhow, bail, Result};
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use plugin_loader::manifest::Backend;
use plugin_loader::LoadedPackage;

use crate::build_processor_with_assets_for_layout;
use crate::processor::{plugin_params_from_set_with_defaults, DEFAULT_PLUGIN_PARAMS};

/// Build a [`BlockProcessor`] from a disk-backed NAM package.
///
/// A grid pedal's knobs map to the NEAREST declared `.nam` capture, and the
/// model is ALWAYS loaded — including when a knob sits at the axis minimum.
/// A capture declared at `drive: 0` (or `level: 0`) is a real capture, not
/// "off", so the block produces sound there. On/off is exclusively the
/// engine's block enable toggle (`set_block_enabled` →
/// `RuntimeProcessor::Bypass`), never a value of a parameter knob.
///
/// `resolve_capture` picks the closest grid point and the model processes
/// input at unity. No artificial dB derivation, no soft-clip, no offline
/// normalization. (Issue #630 removed the legacy #402 "knob at zero ==
/// passthrough" rule, which silently unloaded the model and could not be
/// recovered via the enable toggle.)
pub fn build_from_package(
    package: &LoadedPackage,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let (parameters, captures) = match &package.manifest.backend {
        Backend::Nam {
            parameters,
            captures,
        } => (parameters, captures),
        _ => bail!(
            "nam::build_from_package called with non-NAM backend (model `{}`)",
            package.manifest.id
        ),
    };

    let capture = plugin_loader::dispatch::resolve_capture(parameters, captures, params)
        .ok_or_else(|| {
            anyhow!(
                "no NAM capture matches user params for `{}`",
                package.manifest.id
            )
        })?;
    let model_path = package.root.join(&capture.file);
    let model_path_str = model_path
        .to_str()
        .ok_or_else(|| anyhow!("non-utf8 capture path: {model_path:?}"))?;
    let mut plugin_params = plugin_params_from_set_with_defaults(params, DEFAULT_PLUGIN_PARAMS)?;
    // Audit baseline now lives in the preset's user-facing
    // `output_db`, not stacked on top of it at load time. Whoever
    // creates the block (factory) or migrates an existing preset
    // copies `manifest.output_gain_db` into the param so the UI knob
    // mirrors the engine's actual offset. See `resolve_user_output_level_db`.
    plugin_params.output_level_db = resolve_user_output_level_db(
        plugin_params.output_level_db,
        package.manifest.output_gain_db,
    );
    plugin_params.audit_overrides_baked_output = true;
    build_processor_with_assets_for_layout(model_path_str, None, plugin_params, sample_rate, layout)
}

/// Pure resolver for the runtime output-level offset. The audit
/// (`manifest.output_gain_db`) is no longer stacked on top of the
/// user param — it must already live in `params.output_db` (the
/// block factory writes it there on creation, and the project
/// migration backfills existing presets). This function exists so
/// the contract is testable in isolation from a real model file.
pub fn resolve_user_output_level_db(user_param_db: f32, _manifest_audit_db: Option<f32>) -> f32 {
    user_param_db
}

/// Register this crate's builder in the global package-builders table.
pub fn register_builder() {
    plugin_loader::package_builders::register(
        plugin_loader::package_builders::BackendKind::Nam,
        build_from_package,
    );
}

#[cfg(test)]
#[path = "from_package_tests.rs"]
mod tests;
