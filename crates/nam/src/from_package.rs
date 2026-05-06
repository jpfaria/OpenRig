//! Generic NAM instantiation from a `plugin_loader::LoadedPackage`.
//!
//! Picks the capture file that matches the user's `ParameterSet` (axes
//! declared in the manifest) and hands it to the existing
//! [`crate::build_processor_with_assets_for_layout`].
//!
//! Issue: #287

use anyhow::{anyhow, bail, Result};
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use plugin_loader::manifest::Backend;
use plugin_loader::LoadedPackage;

use crate::build_processor_with_assets_for_layout;
use crate::processor::{plugin_params_from_set_with_defaults, DEFAULT_PLUGIN_PARAMS};

/// Build a [`BlockProcessor`] from a disk-backed NAM package.
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
    let plugin_params = plugin_params_from_set_with_defaults(params, DEFAULT_PLUGIN_PARAMS)?;
    build_processor_with_assets_for_layout(model_path_str, None, plugin_params, sample_rate, layout)
}

/// Register this crate's builder in the global package-builders table.
pub fn register_builder() {
    plugin_loader::package_builders::register(
        plugin_loader::package_builders::BackendKind::Nam,
        build_from_package,
    );
}
