pub mod processor;

use anyhow::{bail, Result};
use processor::{model_schema, params_from_set, supports_model, NamProcessor, DEFAULT_NAM_MODEL};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub fn nam_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema())
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
    if model != DEFAULT_NAM_MODEL {
        bail!("unsupported nam model '{}'", model);
    }
    match layout {
        AudioChannelLayout::Mono => {
            let (model_path, ir_path, plugin_params) = params_from_set(params)?;
            Ok(StageProcessor::Mono(Box::new(NamProcessor::new(
                &model_path,
                ir_path.as_deref(),
                plugin_params,
            )?)))
        }
        AudioChannelLayout::Stereo => bail!(
            "nam model '{}' is mono-only and cannot build native stereo processing",
            model
        ),
    }
}
