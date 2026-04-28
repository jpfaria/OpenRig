use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fender_princeton_reverb";
pub const DISPLAY_NAME: &str = "Princeton Reverb";
const BRAND: &str = "fender";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("eob_vol_5_m160", "Fender - Princeton EOB Vol 5 M160", "amps/fender_princeton_reverb/fender_princeton_eob_vol_5_m160_2.nam"),
    ("eob_vol_5_sum_m160_sm57", "Fender - Princeton EOB Vol 5 SUM M160 + SM57", "amps/fender_princeton_reverb/fender_princeton_eob_vol_5_sum_m160_sm57_2.nam"),
    ("crunch_vol_7_sm57", "Fender - Princeton Crunch Vol 7 SM57", "amps/fender_princeton_reverb/fender_princeton_crunch_vol_7_sm57_2.nam"),
    ("crunch_7_sum_m160_sm57", "Fender - Princeton Crunch 7 SUM M160 + SM57", "amps/fender_princeton_reverb/fender_princeton_crunch_7_sum_m160_sm57_2.nam"),
    ("eob_vol_5_sm57", "Fender - Princeton EOB Vol 5 SM57", "amps/fender_princeton_reverb/fender_princeton_eob_vol_5_sm57_2.nam"),
    ("clean_3_m160", "Fender - Princeton Clean 3 M160", "amps/fender_princeton_reverb/fender_princeton_clean_3_m160_2.nam"),
    ("clean_3_sm57", "Fender - Princeton Clean 3 SM57", "amps/fender_princeton_reverb/fender_princeton_clean_3_sm57_2.nam"),
    ("clean_3_sum_m160_sm57", "Fender - Princeton Clean 3 SUM M160 + SM57", "amps/fender_princeton_reverb/fender_princeton_clean_3_sum_m160_sm57_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("eob_vol_5_m160"),
        &[
            ("eob_vol_5_m160", "Fender - Princeton EOB Vol 5 M160"),
            ("eob_vol_5_sum_m160_sm57", "Fender - Princeton EOB Vol 5 SUM M160 + SM57"),
            ("crunch_vol_7_sm57", "Fender - Princeton Crunch Vol 7 SM57"),
            ("crunch_7_sum_m160_sm57", "Fender - Princeton Crunch 7 SUM M160 + SM57"),
            ("eob_vol_5_sm57", "Fender - Princeton EOB Vol 5 SM57"),
            ("clean_3_m160", "Fender - Princeton Clean 3 M160"),
            ("clean_3_sm57", "Fender - Princeton Clean 3 SM57"),
            ("clean_3_sum_m160_sm57", "Fender - Princeton Clean 3 SUM M160 + SM57"),
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
