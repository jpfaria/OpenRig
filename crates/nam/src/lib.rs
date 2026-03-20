pub mod processor;

use anyhow::{bail, Result};
use processor::{params_from_set, NamPluginParams, NamProcessor};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

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

pub fn build_processor(params: &ParameterSet) -> Result<BlockProcessor> {
    build_processor_for_layout(params, AudioChannelLayout::Mono)
}

pub fn build_processor_for_layout(
    params: &ParameterSet,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let (model_path, ir_path, plugin_params) = params_from_set(params)?;
    build_processor_with_assets_for_layout(&model_path, ir_path.as_deref(), plugin_params, layout)
}

pub fn build_processor_with_assets_for_layout(
    model_path: &str,
    ir_path: Option<&str>,
    plugin_params: NamPluginParams,
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
