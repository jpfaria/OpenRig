use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "boss_hm2_1986";
pub const DISPLAY_NAME: &str = "Heavy Metal HM-2 '86";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct Hm2Capture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[Hm2Capture] = &[
    Hm2Capture { tone: "chainsaw_0gain", model_path: "pedals/boss_hm2_1986/hm2_chainsaw_0gain.nam" },
    Hm2Capture { tone: "chainsaw",       model_path: "pedals/boss_hm2_1986/hm2_chainsaw.nam" },
    Hm2Capture { tone: "medium",         model_path: "pedals/boss_hm2_1986/hm2_medium.nam" },
    Hm2Capture { tone: "warm",           model_path: "pedals/boss_hm2_1986/hm2_warm.nam" },
    Hm2Capture { tone: "bright",         model_path: "pedals/boss_hm2_1986/hm2_bright.nam" },
    Hm2Capture { tone: "high_gain",      model_path: "pedals/boss_hm2_1986/hm2_high_gain.nam" },
    Hm2Capture { tone: "full",           model_path: "pedals/boss_hm2_1986/hm2_full.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("chainsaw"),
        &[
            ("chainsaw_0gain", "Chainsaw (0 Gain)"),
            ("chainsaw",       "Chainsaw"),
            ("medium",         "Medium"),
            ("warm",           "Warm"),
            ("bright",         "Bright"),
            ("high_gain",      "High Gain"),
            ("full",           "Full"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static Hm2Capture> {
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
