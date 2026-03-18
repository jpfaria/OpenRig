use anyhow::{bail, Result};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub fn amp_combo_model_schema(model: &str) -> Result<ModelParameterSchema> {
    bail!("unsupported amp-combo model '{}'", model)
}

pub fn amp_combo_asset_summary(model: &str, _params: &ParameterSet) -> Result<String> {
    bail!("unsupported amp-combo model '{}'", model)
}

pub fn validate_amp_combo_params(model: &str, _params: &ParameterSet) -> Result<()> {
    bail!("unsupported amp-combo model '{}'", model)
}

pub fn build_amp_combo_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_amp_combo_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_amp_combo_processor_for_layout(
    model: &str,
    _params: &ParameterSet,
    _sample_rate: f32,
    _layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    bail!("unsupported amp-combo model '{}'", model)
}
