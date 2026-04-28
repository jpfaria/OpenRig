use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_1959hw_handwired";
pub const DISPLAY_NAME: &str = "1959HW Handwired";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("slammin_plexi_cranked_7_1500e_st", "Slammin_Plexi_Cranked_7_1500E_Standard", "amps/marshall_1959hw_handwired/slammin_plexi_cranked_7_1500e_standard.nam"),
    ("slammin_plexi_cranked_7_2000e_st", "Slammin_Plexi_Cranked_7_2000E_Standard", "amps/marshall_1959hw_handwired/slammin_plexi_cranked_7_2000e_standard.nam"),
    ("slammin_plexi_cranked_6_1700e_co_custom", "Slammin_Plexi_Cranked_6_1700E_Complex", "amps/marshall_1959hw_handwired/slammin_plexi_cranked_6_1700e_complex_custom.nam"),
    ("slammin_plexi_cranked_7_2000e_co_custom", "Slammin_Plexi_Cranked_7_2000E_Complex", "amps/marshall_1959hw_handwired/slammin_plexi_cranked_7_2000e_complex_custom.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("slammin_plexi_cranked_7_1500e_st"),
        &[
            ("slammin_plexi_cranked_7_1500e_st", "Slammin_Plexi_Cranked_7_1500E_Standard"),
            ("slammin_plexi_cranked_7_2000e_st", "Slammin_Plexi_Cranked_7_2000E_Standard"),
            ("slammin_plexi_cranked_6_1700e_co_custom", "Slammin_Plexi_Cranked_6_1700E_Complex"),
            ("slammin_plexi_cranked_7_2000e_co_custom", "Slammin_Plexi_Cranked_7_2000E_Complex"),
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
