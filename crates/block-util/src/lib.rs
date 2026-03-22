//! Utility implementations.
mod processor;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::ModelVisualData;

pub use processor::TunerProcessor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UtilBackendKind {
    Native,
    Nam,
    Ir,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn util_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            UtilBackendKind::Native => "NATIVE",
            UtilBackendKind::Nam => "NAM",
            UtilBackendKind::Ir => "IR",
        },
        panel_bg: def.panel_bg,
        panel_text: def.panel_text,
        brand_strip_bg: def.brand_strip_bg,
        model_font: def.model_font,
    })
}

pub fn utility_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_utility_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: usize,
) -> Result<Box<dyn TunerProcessor>> {
    (registry::find_model_definition(model)?.build)(params, sample_rate)
}
