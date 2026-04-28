use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fender_57_custom_deluxe";
pub const DISPLAY_NAME: &str = "57 Custom Deluxe";
const BRAND: &str = "fender";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("fender_57customdeluxe_od_in1_130", "Fender_57CustomDeluxe_od_in1_1300epochs", "amps/fender_57_custom_deluxe/fender_57customdeluxe_od_in1_1300epochs.nam"),
    ("fender_57customdeluxe_clean_in1_", "Fender_57CustomDeluxe_clean_in1_700epochs", "amps/fender_57_custom_deluxe/fender_57customdeluxe_clean_in1_700epochs.nam"),
    ("fender_57customdeluxe_crunch_in1", "Fender_57CustomDeluxe_crunch_in1_1000epochs", "amps/fender_57_custom_deluxe/fender_57customdeluxe_crunch_in1_1000epochs.nam"),
    ("fender_57customdeluxe_clean_in2_", "Fender_57CustomDeluxe_clean_in2_700epochs", "amps/fender_57_custom_deluxe/fender_57customdeluxe_clean_in2_700epochs.nam"),
    ("fender_57customdeluxe_crunch_in2", "Fender_57CustomDeluxe_crunch_in2_1000epochs", "amps/fender_57_custom_deluxe/fender_57customdeluxe_crunch_in2_1000epochs.nam"),
    ("fender_57customdeluxe_od_in2_130", "Fender_57CustomDeluxe_od_in2_1300epochs", "amps/fender_57_custom_deluxe/fender_57customdeluxe_od_in2_1300epochs.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("fender_57customdeluxe_od_in1_130"),
        &[
            ("fender_57customdeluxe_od_in1_130", "Fender_57CustomDeluxe_od_in1_1300epochs"),
            ("fender_57customdeluxe_clean_in1_", "Fender_57CustomDeluxe_clean_in1_700epochs"),
            ("fender_57customdeluxe_crunch_in1", "Fender_57CustomDeluxe_crunch_in1_1000epochs"),
            ("fender_57customdeluxe_clean_in2_", "Fender_57CustomDeluxe_clean_in2_700epochs"),
            ("fender_57customdeluxe_crunch_in2", "Fender_57CustomDeluxe_crunch_in2_1000epochs"),
            ("fender_57customdeluxe_od_in2_130", "Fender_57CustomDeluxe_od_in2_1300epochs"),
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
