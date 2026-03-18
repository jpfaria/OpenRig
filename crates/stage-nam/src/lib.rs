use anyhow::{bail, Result};
use nam::{build_processor_for_layout, model_schema_for};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub use nam::GENERIC_NAM_MODEL_ID;

pub fn supports_model(model: &str) -> bool {
    model == GENERIC_NAM_MODEL_ID
}

pub fn nam_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema_for(
            "nam",
            GENERIC_NAM_MODEL_ID,
            "Neural Amp Modeler",
            true,
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
