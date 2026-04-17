use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "sansamp_di_2112";
pub const DISPLAY_NAME: &str = "SansAmp DI-2112";
const BRAND: &str = "tech21";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct Di2112Capture {
    preset: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[Di2112Capture] = &[
    Di2112Capture { preset: "geddy_standard",    model_path: "pedals/sansamp_di_2112/di2112_geddy_standard.nam" },
    Di2112Capture { preset: "geddy_roundabout",  model_path: "pedals/sansamp_di_2112/di2112_geddy_roundabout.nam" },
    Di2112Capture { preset: "yyz",               model_path: "pedals/sansamp_di_2112/di2112_yyz.nam" },
    Di2112Capture { preset: "jack_bruce",        model_path: "pedals/sansamp_di_2112/di2112_jack_bruce.nam" },
    Di2112Capture { preset: "jpj",               model_path: "pedals/sansamp_di_2112/di2112_jpj.nam" },
    Di2112Capture { preset: "les_claypool",      model_path: "pedals/sansamp_di_2112/di2112_les_claypool.nam" },
    Di2112Capture { preset: "entwistle",         model_path: "pedals/sansamp_di_2112/di2112_entwistle.nam" },
    Di2112Capture { preset: "radiohead",         model_path: "pedals/sansamp_di_2112/di2112_radiohead.nam" },
    Di2112Capture { preset: "deep_sat",          model_path: "pedals/sansamp_di_2112/di2112_deep_sat.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Pedal"),
        Some("geddy_standard"),
        &[
            ("geddy_standard",   "Geddy Standard"),
            ("geddy_roundabout", "Geddy Roundabout"),
            ("yyz",              "YYZ"),
            ("jack_bruce",       "Jack Bruce"),
            ("jpj",              "JPJ"),
            ("les_claypool",     "Les Claypool"),
            ("entwistle",        "Entwistle Leeds"),
            ("radiohead",        "Radiohead National Anthem"),
            ("deep_sat",         "Deep Saturation"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static Di2112Capture> {
    let preset = required_string(params, "preset").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.preset == preset)
        .ok_or_else(|| anyhow!("gain model '{}' does not support preset='{}'", MODEL_ID, preset))
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
