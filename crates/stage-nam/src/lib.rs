use anyhow::{bail, Result};
use nam::{build_processor_for_layout, model_schema_for, GENERIC_NAM_MODEL_ID};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub const DEFAULT_NAM_MODEL: &str = GENERIC_NAM_MODEL_ID;

pub fn supports_model(model: &str) -> bool {
    model == DEFAULT_NAM_MODEL
}

pub fn nam_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema_for(
            "nam",
            DEFAULT_NAM_MODEL,
            "Neural Amp Modeler",
        ))
    } else {
        bail!("unsupported nam model '{}'", model)
    }
}

pub fn build_nam_processor(model: &str, params: &ParameterSet) -> Result<StageProcessor> {
    build_nam_processor_for_layout(model, params, AudioChannelLayout::Mono)
}

pub fn build_nam_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if !supports_model(model) {
        bail!("unsupported nam model '{}'", model);
    }
    build_processor_for_layout(params, layout)
}
