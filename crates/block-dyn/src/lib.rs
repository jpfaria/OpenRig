//! Dynamics implementations.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DynBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn dyn_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            DynBackendKind::Native => "NATIVE",
            DynBackendKind::Nam => "NAM",
            DynBackendKind::Ir => "IR",
            DynBackendKind::Lv2 => "LV2",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn dyn_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.display_name).unwrap_or("")
}

pub fn dyn_brand(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.brand).unwrap_or("")
}

pub fn dyn_type_label(model: &str) -> &'static str {
    dyn_model_visual(model).map(|v| v.type_label).unwrap_or("")
}

pub fn compressor_supported_models() -> &'static [&'static str] {
    registry::COMPRESSOR_SUPPORTED_MODELS
}

pub fn gate_supported_models() -> &'static [&'static str] {
    registry::GATE_SUPPORTED_MODELS
}

pub fn dynamics_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_dynamics_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_dynamics_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_dynamics_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

pub fn compressor_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_compressor_model_definition(model)?.schema)()
}

pub fn build_compressor_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_compressor_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_compressor_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_compressor_model_definition(model)?.build)(params, sample_rate, layout)
}

pub fn gate_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_gate_model_definition(model)?.schema)()
}

pub fn build_gate_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_gate_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_gate_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_gate_model_definition(model)?.build)(params, sample_rate, layout)
}
