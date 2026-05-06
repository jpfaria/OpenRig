pub mod from_package;
pub mod processor;

pub use from_package::{build_from_package, register_builder};

use anyhow::{bail, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};
use processor::{params_from_set, NamPluginParams, NamProcessor};

pub const GENERIC_NAM_MODEL_ID: &str = "neural_amp_modeler";

pub fn model_schema_for(
    effect_type: &str,
    model: &str,
    display_name: &str,
    include_file_params: bool,
) -> ModelParameterSchema {
    let mut schema = processor::model_schema(include_file_params);
    schema.effect_type = effect_type.to_string();
    schema.model = model.to_string();
    schema.display_name = display_name.to_string();
    schema
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<BlockProcessor> {
    build_processor_for_layout(params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_processor_for_layout(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let (model_path, ir_path, plugin_params) = params_from_set(params)?;
    build_processor_with_assets_for_layout(
        &model_path,
        ir_path.as_deref(),
        plugin_params,
        sample_rate,
        layout,
    )
}

pub fn build_processor_with_assets_for_layout(
    model_path: &str,
    ir_path: Option<&str>,
    plugin_params: NamPluginParams,
    _sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(NamProcessor::new(
            model_path,
            ir_path,
            plugin_params,
        )?))),
        AudioChannelLayout::Stereo => {
            bail!("the NAM processor is mono-native and cannot build native stereo processing")
        }
    }
}

// resolve_nam_capture removed in issue #287: its only callers were the
// per-plugin `nam_*.rs` files in crates/block-*/src/, which moved to
// OpenRig-plugins. Plugin-loader resolves capture paths relative to the
// loaded package root.

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
