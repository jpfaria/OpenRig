use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_boss_hm_3";
pub const DISPLAY_NAME: &str = "Boss HM-3";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "lows10_highs10_dist10_boss_hm3", model_path: "pedals/boss_hm_3/boss_hm3_lows10_highs10_dist10_boss_hm3.nam" },
    NamCapture { tone: "lows10_highs10_dist5_boss_hm3",  model_path: "pedals/boss_hm_3/boss_hm3_lows10_highs10_dist5_boss_hm3.nam" },
    NamCapture { tone: "lows10_highs5_dist10_boss_hm3",  model_path: "pedals/boss_hm_3/boss_hm3_lows10_highs5_dist10_boss_hm3.nam" },
    NamCapture { tone: "lows10_highs5_dist5_boss_hm3",   model_path: "pedals/boss_hm_3/boss_hm3_lows10_highs5_dist5_boss_hm3.nam" },
    NamCapture { tone: "lows5_highs10_dist10_boss_hm3",  model_path: "pedals/boss_hm_3/boss_hm3_lows5_highs10_dist10_boss_hm3.nam" },
    NamCapture { tone: "lows5_highs10_dist5_boss_hm3",   model_path: "pedals/boss_hm_3/boss_hm3_lows5_highs10_dist5_boss_hm3.nam" },
    NamCapture { tone: "lows5_highs5_dist10_boss_hm3",   model_path: "pedals/boss_hm_3/boss_hm3_lows5_highs5_dist10_boss_hm3.nam" },
    NamCapture { tone: "lows5_highs5_dist5_boss_hm3",    model_path: "pedals/boss_hm_3/boss_hm3_lows5_highs5_dist5_boss_hm3.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("lows10_highs10_dist10_boss_hm3"),
        &[
            ("lows10_highs10_dist10_boss_hm3", "Lows10 Highs10 Dist10 Boss Hm3"),
            ("lows10_highs10_dist5_boss_hm3",  "Lows10 Highs10 Dist5 Boss Hm3"),
            ("lows10_highs5_dist10_boss_hm3",  "Lows10 Highs5 Dist10 Boss Hm3"),
            ("lows10_highs5_dist5_boss_hm3",   "Lows10 Highs5 Dist5 Boss Hm3"),
            ("lows5_highs10_dist10_boss_hm3",  "Lows5 Highs10 Dist10 Boss Hm3"),
            ("lows5_highs10_dist5_boss_hm3",   "Lows5 Highs10 Dist5 Boss Hm3"),
            ("lows5_highs5_dist10_boss_hm3",   "Lows5 Highs5 Dist10 Boss Hm3"),
            ("lows5_highs5_dist5_boss_hm3",    "Lows5 Highs5 Dist5 Boss Hm3"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static NamCapture> {
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.tone == tone)
        .ok_or_else(|| anyhow!("gain model '{}' does not support tone='{}'", MODEL_ID, tone))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
