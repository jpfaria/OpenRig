use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_jcm2000_dsl";
pub const DISPLAY_NAME: &str = "JCM2000 DSL";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("od2_altrock_esr0_0050", "BMR - Marshall JCM2000 OD2 AltRock - ESR0-0050", "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_od2_altrock_esr0_0050_2.nam"),
    ("clean_esr0_0020", "BMR - Marshall JCM2000 Clean - ESR0-0020", "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_clean_esr0_0020_2.nam"),
    ("od2_dimed_esr0_0813", "BMR - Marshall JCM2000 OD2 DIMED - ESR0-0813", "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_od2_dimed_esr0_0813_2.nam"),
    ("od1_dimed_esr0_0357", "BMR - Marshall JCM2000 OD1 DIMED - ESR0-0357", "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_od1_dimed_esr0_0357_2.nam"),
    ("od1_altrock_esr0_0055", "BMR - Marshall JCM2000 OD1 AltRock - ESR0-0055", "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_od1_altrock_esr0_0055_2.nam"),
    ("crunch_esr0_0021", "BMR - Marshall JCM2000 Crunch - ESR0-0021", "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_crunch_esr0_0021_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("od2_altrock_esr0_0050"),
        &[
            ("od2_altrock_esr0_0050", "BMR - Marshall JCM2000 OD2 AltRock - ESR0-0050"),
            ("clean_esr0_0020", "BMR - Marshall JCM2000 Clean - ESR0-0020"),
            ("od2_dimed_esr0_0813", "BMR - Marshall JCM2000 OD2 DIMED - ESR0-0813"),
            ("od1_dimed_esr0_0357", "BMR - Marshall JCM2000 OD1 DIMED - ESR0-0357"),
            ("od1_altrock_esr0_0055", "BMR - Marshall JCM2000 OD1 AltRock - ESR0-0055"),
            ("crunch_esr0_0021", "BMR - Marshall JCM2000 Crunch - ESR0-0021"),
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
