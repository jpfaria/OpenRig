use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_evh_5150_iii";
pub const DISPLAY_NAME: &str = "5150 III";
const BRAND: &str = "evh";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("ob1_evh_blue_fortin_33_full_rig", "OB1 EVH Blue Fortin 33 - Full Rig", "amps/evh_5150_iii/ob1_evh_blue_fortin_33_full_rig.nam"),
    ("ob1_evh_blue_805_full_rig", "OB1 EVH Blue 805 - Full Rig", "amps/evh_5150_iii/ob1_evh_blue_805_full_rig.nam"),
    ("ob1_evh_red_tc_spark_full_rig", "OB1 EVH Red TC Spark - Full Rig", "amps/evh_5150_iii/ob1_evh_red_tc_spark_full_rig.nam"),
    ("ob1_evh_red_fortin_33_full_rig", "OB1 EVH Red Fortin 33 - Full Rig", "amps/evh_5150_iii/ob1_evh_red_fortin_33_full_rig.nam"),
    ("ob1_evh_red_precision_drive_full", "OB1 EVH Red Precision Drive - Full Rig", "amps/evh_5150_iii/ob1_evh_red_precision_drive_full_rig.nam"),
    ("ob1_evh_blue_precision_drive_ful", "OB1 EVH Blue Precision Drive - Full Rig", "amps/evh_5150_iii/ob1_evh_blue_precision_drive_full_rig.nam"),
    ("ob1_evh_blue_tc_spark_full_rig", "OB1 EVH Blue TC Spark - Full Rig", "amps/evh_5150_iii/ob1_evh_blue_tc_spark_full_rig.nam"),
    ("ob1_evh_red_805_full_rig", "OB1 EVH Red 805 - Full Rig", "amps/evh_5150_iii/ob1_evh_red_805_full_rig.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("ob1_evh_blue_fortin_33_full_rig"),
        &[
            ("ob1_evh_blue_fortin_33_full_rig", "OB1 EVH Blue Fortin 33 - Full Rig"),
            ("ob1_evh_blue_805_full_rig", "OB1 EVH Blue 805 - Full Rig"),
            ("ob1_evh_red_tc_spark_full_rig", "OB1 EVH Red TC Spark - Full Rig"),
            ("ob1_evh_red_fortin_33_full_rig", "OB1 EVH Red Fortin 33 - Full Rig"),
            ("ob1_evh_red_precision_drive_full", "OB1 EVH Red Precision Drive - Full Rig"),
            ("ob1_evh_blue_precision_drive_ful", "OB1 EVH Blue Precision Drive - Full Rig"),
            ("ob1_evh_blue_tc_spark_full_rig", "OB1 EVH Blue TC Spark - Full Rig"),
            ("ob1_evh_red_805_full_rig", "OB1 EVH Red 805 - Full Rig"),
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
