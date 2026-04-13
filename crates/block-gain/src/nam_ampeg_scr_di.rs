use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "ampeg_scr_di";
pub const DISPLAY_NAME: &str = "SCR-DI";
const BRAND: &str = "ampeg";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct ScrDiCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[ScrDiCapture] = &[
    ScrDiCapture { tone: "standard",      model_path: "pedals/ampeg_scr_di/scr_di_standard.nam" },
    ScrDiCapture { tone: "ultra_lo",      model_path: "pedals/ampeg_scr_di/scr_di_ultra_lo.nam" },
    ScrDiCapture { tone: "ultra_hi",      model_path: "pedals/ampeg_scr_di/scr_di_ultra_hi.nam" },
    ScrDiCapture { tone: "ultra_lo_hi",   model_path: "pedals/ampeg_scr_di/scr_di_ultra_lo_hi.nam" },
    ScrDiCapture { tone: "scrambler_med", model_path: "pedals/ampeg_scr_di/scr_di_scrambler_med.nam" },
    ScrDiCapture { tone: "scrambler_max", model_path: "pedals/ampeg_scr_di/scr_di_scrambler_max.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("standard"),
        &[
            ("standard",      "Standard"),
            ("ultra_lo",      "Ultra Lo"),
            ("ultra_hi",      "Ultra Hi"),
            ("ultra_lo_hi",   "Ultra Lo+Hi"),
            ("scrambler_med", "Scrambler Med"),
            ("scrambler_max", "Scrambler Max"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static ScrDiCapture> {
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
