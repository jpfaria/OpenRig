use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "fulltone_ocd";
pub const DISPLAY_NAME: &str = "OCD Overdrive";
const BRAND: &str = "fulltone";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcdParams {
    pub mode: &'static str,
    pub drive: &'static str,
}

struct OcdCapture {
    params: OcdParams,
    model_path: &'static str,
}

const CAPTURES: &[OcdCapture] = &[
    OcdCapture { params: OcdParams { mode: "lp", drive: "0" }, model_path: "pedals/fulltone_ocd/ocd_lp_d0.nam" },
    OcdCapture { params: OcdParams { mode: "lp", drive: "4" }, model_path: "pedals/fulltone_ocd/ocd_lp_d4.nam" },
    OcdCapture { params: OcdParams { mode: "lp", drive: "7" }, model_path: "pedals/fulltone_ocd/ocd_lp_d7.nam" },
    OcdCapture { params: OcdParams { mode: "hp", drive: "0" }, model_path: "pedals/fulltone_ocd/ocd_hp_d0.nam" },
    OcdCapture { params: OcdParams { mode: "hp", drive: "4" }, model_path: "pedals/fulltone_ocd/ocd_hp_d4.nam" },
    OcdCapture { params: OcdParams { mode: "hp", drive: "7" }, model_path: "pedals/fulltone_ocd/ocd_hp_d7.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "mode",
            "Mode",
            Some("Pedal"),
            Some("lp"),
            &[("lp", "LP"), ("hp", "HP")],
        ),
        enum_parameter(
            "drive",
            "Drive",
            Some("Pedal"),
            Some("4"),
            &[("0", "Low"), ("4", "Medium"), ("7", "High")],
        ),
    ];
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static OcdCapture> {
    let mode = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    let drive = required_string(params, "drive").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.params.mode == mode && c.params.drive == drive)
        .ok_or_else(|| {
            anyhow!(
                "gain model '{}' does not support mode='{}' drive='{}'",
                MODEL_ID, mode, drive
            )
        })
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
