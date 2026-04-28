use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_vox_ac15";
pub const DISPLAY_NAME: &str = "AC15";
const BRAND: &str = "vox";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("vox_ac15ch_edge_of_breakup_norma", "Vox AC15CH Edge of Breakup Normal", "amps/vox_ac15/vox_ac15ch_edge_of_breakup_normal.nam"),
    ("vox_ac15ch_crunch_normal", "Vox AC15CH Crunch Normal", "amps/vox_ac15/vox_ac15ch_crunch_normal.nam"),
    ("vox_ac15ch_overdriven_normal", "Vox AC15CH Overdriven Normal", "amps/vox_ac15/vox_ac15ch_overdriven_normal.nam"),
    ("vox_ac15ch_crystal_clean_normal", "Vox AC15CH Crystal Clean Normal", "amps/vox_ac15/vox_ac15ch_crystal_clean_normal.nam"),
    ("vox_ac15ch_edge_of_breakup_tb", "Vox AC15CH Edge of Breakup TB", "amps/vox_ac15/vox_ac15ch_edge_of_breakup_tb.nam"),
    ("vox_ac15ch_crunch_tb", "Vox AC15CH Crunch TB", "amps/vox_ac15/vox_ac15ch_crunch_tb.nam"),
    ("vox_ac15ch_crystal_clean_tb", "Vox AC15CH Crystal Clean TB", "amps/vox_ac15/vox_ac15ch_crystal_clean_tb.nam"),
    ("vox_ac15ch_overdriven_tb", "Vox AC15CH Overdriven TB", "amps/vox_ac15/vox_ac15ch_overdriven_tb.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("vox_ac15ch_edge_of_breakup_norma"),
        &[
            ("vox_ac15ch_edge_of_breakup_norma", "Vox AC15CH Edge of Breakup Normal"),
            ("vox_ac15ch_crunch_normal", "Vox AC15CH Crunch Normal"),
            ("vox_ac15ch_overdriven_normal", "Vox AC15CH Overdriven Normal"),
            ("vox_ac15ch_crystal_clean_normal", "Vox AC15CH Crystal Clean Normal"),
            ("vox_ac15ch_edge_of_breakup_tb", "Vox AC15CH Edge of Breakup TB"),
            ("vox_ac15ch_crunch_tb", "Vox AC15CH Crunch TB"),
            ("vox_ac15ch_crystal_clean_tb", "Vox AC15CH Crystal Clean TB"),
            ("vox_ac15ch_overdriven_tb", "Vox AC15CH Overdriven TB"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let path = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no capture '{}'", MODEL_ID, key))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: AmpBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}
