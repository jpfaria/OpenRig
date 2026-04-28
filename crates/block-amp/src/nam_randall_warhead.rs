use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_randall_warhead";
pub const DISPLAY_NAME: &str = "Warhead";
const BRAND: &str = "randall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("x2_dimezone_v30_tlm102", "X2-DimeZone-v30-tlm102", "amps/randall_warhead/x2_dimezone_v30_tlm102_2.nam"),
    ("x2_dimezone_jaguar_mixed", "X2-DimeZone-jaguar-mixed", "amps/randall_warhead/x2_dimezone_jaguar_mixed_2.nam"),
    ("x2_dimezone_v30_sm57", "X2-DimeZone-v30-sm57", "amps/randall_warhead/x2_dimezone_v30_sm57_2.nam"),
    ("x2_dimezone_jaguar_tlm102", "X2-DimeZone-jaguar-tlm102", "amps/randall_warhead/x2_dimezone_jaguar_tlm102_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("x2_dimezone_v30_tlm102"),
        &[
            ("x2_dimezone_v30_tlm102", "X2-DimeZone-v30-tlm102"),
            ("x2_dimezone_jaguar_mixed", "X2-DimeZone-jaguar-mixed"),
            ("x2_dimezone_v30_sm57", "X2-DimeZone-v30-sm57"),
            ("x2_dimezone_jaguar_tlm102", "X2-DimeZone-jaguar-tlm102"),
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
