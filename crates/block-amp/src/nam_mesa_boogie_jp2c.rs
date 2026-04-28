use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_boogie_jp2c";
pub const DISPLAY_NAME: &str = "JP2C";
const BRAND: &str = "mesa";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("yellow_od808", "Yellow OD808 Marshall Full Rig - jp_is_out_of_tune", "amps/mesa_boogie_jp2c/jp2c_yellow_od808_marshall_full_rig_jp_is_out_of_tune.nam"),
    ("red_od808", "Red OD808 Marshall Full Rig - jp_is_out_of_tune", "amps/mesa_boogie_jp2c/jp2c_red_od808_marshall_full_rig_jp_is_out_of_tune.nam"),
    ("red", "Red Marshall Full Rig - jp_is_out_of_tune", "amps/mesa_boogie_jp2c/jp2c_red_marshall_full_rig_jp_is_out_of_tune.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("yellow_od808"),
        &[
            ("yellow_od808", "Yellow OD808 Marshall Full Rig - jp_is_out_of_tune"),
            ("red_od808", "Red OD808 Marshall Full Rig - jp_is_out_of_tune"),
            ("red", "Red Marshall Full Rig - jp_is_out_of_tune"),
        
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
    let key = required_string(params, "preset").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no preset '{}'", MODEL_ID, key))
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
