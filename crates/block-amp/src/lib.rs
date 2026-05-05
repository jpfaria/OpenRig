pub mod model_visual;
pub mod native_core;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn amp_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn amp_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    (registry::find_model_definition(model)?.asset_summary)(params)
}

pub fn validate_amp_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_amp_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_amp_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_amp_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    if let Ok(definition) = registry::find_model_definition(model) {
        return (definition.build)(params, sample_rate, layout);
    }
    if let Some(package) = plugin_loader::registry::find(model) {
        return package.build_processor(params, sample_rate, layout);
    }
    anyhow::bail!("unsupported amp model '{}'", model)
}


pub fn amp_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            registry::AmpBackendKind::Native => "NATIVE",
            registry::AmpBackendKind::Nam => "NAM",
            registry::AmpBackendKind::Ir => "IR",
            registry::AmpBackendKind::Lv2 => "LV2",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn amp_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.display_name).unwrap_or("")
}

pub fn amp_brand(model: &str) -> Result<&'static str> {
    Ok(registry::find_model_definition(model)?.brand)
}

pub fn amp_type_label(model: &str) -> Result<&'static str> {
    match registry::find_model_definition(model)?.backend_kind {
        registry::AmpBackendKind::Native => Ok("NATIVE"),
        registry::AmpBackendKind::Nam => Ok("NAM"),
        registry::AmpBackendKind::Ir => Ok("IR"),
        registry::AmpBackendKind::Lv2 => Ok("LV2"),
    }
}

/// Push every native model into the unified plugin-loader registry.
/// Called by `adapter-gui` at startup before plugin discovery freezes
/// the catalog.
pub fn register_natives() {
    registry::register_natives();
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
