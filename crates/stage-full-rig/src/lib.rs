pub mod roland_jc_120b_jazz_chorus;

use anyhow::{bail, Result};
use roland_jc_120b_jazz_chorus::{
    asset_summary as roland_asset_summary,
    build_processor_for_model as build_roland_processor,
    model_schema as roland_model_schema,
    supports_model as supports_roland_model,
    validate_params as validate_roland_params,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub const DEFAULT_FULL_RIG_MODEL: &str = roland_jc_120b_jazz_chorus::MODEL_ID;

pub fn full_rig_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_roland_model(model) {
        Ok(roland_model_schema())
    } else {
        bail!("unsupported full-rig model '{}'", model)
    }
}

pub fn full_rig_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    if supports_roland_model(model) {
        roland_asset_summary(params)
    } else {
        bail!("unsupported full-rig model '{}'", model)
    }
}

pub fn validate_full_rig_params(model: &str, params: &ParameterSet) -> Result<()> {
    if supports_roland_model(model) {
        validate_roland_params(params)
    } else {
        bail!("unsupported full-rig model '{}'", model)
    }
}

pub fn build_full_rig_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    _sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_roland_model(model) {
        build_roland_processor(params, layout)
    } else {
        bail!("unsupported full-rig model '{}'", model)
    }
}
