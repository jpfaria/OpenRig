use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "boss_hm2_mij";
pub const DISPLAY_NAME: &str = "Heavy Metal HM-2 MiJ";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct Hm2MijCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[Hm2MijCapture] = &[
    Hm2MijCapture { tone: "swede",       model_path: "pedals/boss_hm2_mij/hm2_mij_swede.nam" },
    Hm2MijCapture { tone: "godflesh",    model_path: "pedals/boss_hm2_mij/hm2_mij_godflesh.nam" },
    Hm2MijCapture { tone: "atg",         model_path: "pedals/boss_hm2_mij/hm2_mij_atg.nam" },
    Hm2MijCapture { tone: "boost_sharp", model_path: "pedals/boss_hm2_mij/hm2_mij_boost_sharp.nam" },
    Hm2MijCapture { tone: "boost_blunt", model_path: "pedals/boss_hm2_mij/hm2_mij_boost_blunt.nam" },
    Hm2MijCapture { tone: "boost_che",   model_path: "pedals/boss_hm2_mij/hm2_mij_boost_che.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("swede"),
        &[
            ("swede",       "Swede"),
            ("godflesh",    "Godflesh"),
            ("atg",         "ATG"),
            ("boost_sharp", "Boost Sharp"),
            ("boost_blunt", "Boost Blunt"),
            ("boost_che",   "Boost Che"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static Hm2MijCapture> {
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
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
