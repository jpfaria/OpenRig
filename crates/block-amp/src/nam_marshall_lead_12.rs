use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_lead_12";
pub const DISPLAY_NAME: &str = "Lead 12";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("low_g0_v5_t6_m5_feather", "Marshall Lead 12 Low Input (G0 V5 T6 M5 B7)", "amps/marshall_lead_12/marshall_lead_12_low_input_g0_v5_t6_m5_b7_feather.nam"),
    ("low_g3_v5_t6_m5_feather", "Marshall Lead 12 Low Input (G3 V5 T6 M5 B7)", "amps/marshall_lead_12/marshall_lead_12_low_input_g3_v5_t6_m5_b7_feather.nam"),
    ("high_g8_v5_t7_m4_feather", "Marshall Lead 12 High Input (G8 V5 T7 M4 B7)", "amps/marshall_lead_12/marshall_lead_12_high_input_g8_v5_t7_m4_b7_feather.nam"),
    ("high_g4_v4_t7_m4_feather", "Marshall Lead 12 High Input (G4 V4 T7 M4 B7)", "amps/marshall_lead_12/marshall_lead_12_high_input_g4_v4_t7_m4_b7_feather.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("low_g0_v5_t6_m5_feather"),
        &[
            ("low_g0_v5_t6_m5_feather", "Marshall Lead 12 Low Input (G0 V5 T6 M5 B7)"),
            ("low_g3_v5_t6_m5_feather", "Marshall Lead 12 Low Input (G3 V5 T6 M5 B7)"),
            ("high_g8_v5_t7_m4_feather", "Marshall Lead 12 High Input (G8 V5 T7 M4 B7)"),
            ("high_g4_v4_t7_m4_feather", "Marshall Lead 12 High Input (G4 V4 T7 M4 B7)"),
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
