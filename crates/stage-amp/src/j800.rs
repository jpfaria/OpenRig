use anyhow::Result;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for, processor::plugin_params_from_set,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub const MODEL_ID: &str = "j800";
const MODEL_PATH: &str =
    "captures/nam/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV6 G4 - AZG - 700.nam";
const IR_PATH: &str = "captures/ir/cabs/Marshall 4x12 V30 IR/EV MIX B.wav";

pub fn supports_model(model: &str) -> bool {
    matches!(model, MODEL_ID | "jcm800" | "jcm_800")
}

pub fn model_schema() -> ModelParameterSchema {
    model_schema_for("amp", MODEL_ID, "J800", false)
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    let plugin_params = plugin_params_from_set(params)?;
    build_processor_with_assets_for_layout(MODEL_PATH, Some(IR_PATH), plugin_params, layout)
}

pub fn asset_paths() -> (&'static str, &'static str) {
    (MODEL_PATH, IR_PATH)
}
