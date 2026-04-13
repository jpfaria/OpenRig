pub mod processor;

use anyhow::{bail, Result};
use processor::{params_from_set, NamPluginParams, NamProcessor};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};
use std::path::PathBuf;

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
    build_processor_with_assets_for_layout(&model_path, ir_path.as_deref(), plugin_params, sample_rate, layout)
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

/// Resolve a NAM capture path relative to the configured `nam_captures` root.
///
/// `relative_path` is the portion after `captures/nam/`, e.g.
/// `"amps/dumble/dumble_clean_4x12_v30.nam"`.  The function searches
/// relative to the executable first, then falls back to the config path
/// directly.
pub fn resolve_nam_capture(relative_path: &str) -> Result<String> {
    let paths = infra_filesystem::asset_paths();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../").join(&paths.nam_captures).join(relative_path)),
        Some(PathBuf::from(&paths.nam_captures).join(relative_path)),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    bail!(
        "NAM capture '{}' not found in '{}'",
        relative_path,
        paths.nam_captures
    )
}

