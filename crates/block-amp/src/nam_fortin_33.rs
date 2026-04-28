use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fortin_33";
pub const DISPLAY_NAME: &str = "33";
const BRAND: &str = "fortin";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("blue_fortin_33", "OB1 EVH Blue Fortin 33 - Full Rig", "amps/fortin_33/ob1_evh_blue_fortin_33_full_rig_2.nam"),
    ("blue_805", "OB1 EVH Blue 805 - Full Rig", "amps/fortin_33/ob1_evh_blue_805_full_rig_2.nam"),
    ("red_tc_spark", "OB1 EVH Red TC Spark - Full Rig", "amps/fortin_33/ob1_evh_red_tc_spark_full_rig_2.nam"),
    ("red_fortin_33", "OB1 EVH Red Fortin 33 - Full Rig", "amps/fortin_33/ob1_evh_red_fortin_33_full_rig_2.nam"),
    ("red_precision_drive", "OB1 EVH Red Precision Drive - Full Rig", "amps/fortin_33/ob1_evh_red_precision_drive_full_rig_2.nam"),
    ("blue_precision_drive", "OB1 EVH Blue Precision Drive - Full Rig", "amps/fortin_33/ob1_evh_blue_precision_drive_full_rig_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("blue_fortin_33"),
        &[
            ("blue_fortin_33", "OB1 EVH Blue Fortin 33 - Full Rig"),
            ("blue_805", "OB1 EVH Blue 805 - Full Rig"),
            ("red_tc_spark", "OB1 EVH Red TC Spark - Full Rig"),
            ("red_fortin_33", "OB1 EVH Red Fortin 33 - Full Rig"),
            ("red_precision_drive", "OB1 EVH Red Precision Drive - Full Rig"),
            ("blue_precision_drive", "OB1 EVH Blue Precision Drive - Full Rig"),
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
