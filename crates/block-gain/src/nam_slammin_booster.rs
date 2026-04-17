use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "slammin_booster";
pub const DISPLAY_NAME: &str = "Slammin Clean Booster";
const BRAND: &str = "jhs";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct SlamminCapture {
    character: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[SlamminCapture] = &[
    SlamminCapture { character: "od808_t5",     model_path: "pedals/slammin_booster/slammin_od808_t5.nam" },
    SlamminCapture { character: "od808_t7",     model_path: "pedals/slammin_booster/slammin_od808_t7.nam" },
    SlamminCapture { character: "ocd_lp_t5",    model_path: "pedals/slammin_booster/slammin_ocd_lp_t5.nam" },
    SlamminCapture { character: "ocd_hp_t5",    model_path: "pedals/slammin_booster/slammin_ocd_hp_t5.nam" },
    SlamminCapture { character: "sd1_t5",       model_path: "pedals/slammin_booster/slammin_sd1_t5.nam" },
    SlamminCapture { character: "sd1_t7",       model_path: "pedals/slammin_booster/slammin_sd1_t7.nam" },
    SlamminCapture { character: "goldenpearl_t5", model_path: "pedals/slammin_booster/slammin_goldenpearl_t5.nam" },
    SlamminCapture { character: "echopre_bright", model_path: "pedals/slammin_booster/slammin_echopre_bright.nam" },
    SlamminCapture { character: "echopre_mid",    model_path: "pedals/slammin_booster/slammin_echopre_mid.nam" },
    SlamminCapture { character: "echopre_dark",   model_path: "pedals/slammin_booster/slammin_echopre_dark.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "character",
        "Character",
        Some("Pedal"),
        Some("od808_t5"),
        &[
            ("od808_t5",      "OD808 T5"),
            ("od808_t7",      "OD808 T7"),
            ("ocd_lp_t5",     "OCD LP T5"),
            ("ocd_hp_t5",     "OCD HP T5"),
            ("sd1_t5",        "SD1 T5"),
            ("sd1_t7",        "SD1 T7"),
            ("goldenpearl_t5", "Golden Pearl"),
            ("echopre_bright", "EchoPre Bright"),
            ("echopre_mid",    "EchoPre Mid"),
            ("echopre_dark",   "EchoPre Dark"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static SlamminCapture> {
    let character = required_string(params, "character").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.character == character)
        .ok_or_else(|| anyhow!("gain model '{}' does not support character='{}'", MODEL_ID, character))
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
