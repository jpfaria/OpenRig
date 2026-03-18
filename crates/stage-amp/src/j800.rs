use anyhow::Result;
use nam::{build_processor_for_layout, model_schema_for};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub const MODEL_ID: &str = "j800";

pub fn supports_model(model: &str) -> bool {
    matches!(model, MODEL_ID | "jcm800" | "jcm_800")
}

pub fn model_schema() -> ModelParameterSchema {
    model_schema_for("amp", MODEL_ID, "J800")
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    build_processor_for_layout(params, layout)
}
