use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_jcm900";
pub const DISPLAY_NAME: &str = "JCM900";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("vox_ac15_crsh_2", "Vox AC15 Crsh 2", "amps/marshall_jcm900/vox_ac15_crsh_2.nam"),
    ("jtm_45_crsh", "JTM 45 CRSH", "amps/marshall_jcm900/jtm_45_crsh.nam"),
    ("vox_ac15_crunsh", "Vox AC15 Crunsh", "amps/marshall_jcm900/vox_ac15_crunsh.nam"),
    ("vox_ac15_clean", "Vox AC15 Clean", "amps/marshall_jcm900/vox_ac15_clean.nam"),
    ("marshall_jtm_45_clean", "Marshall JTM 45 Clean", "amps/marshall_jcm900/marshall_jtm_45_clean.nam"),
    ("ods_dumble_clean", "ODS Dumble clean", "amps/marshall_jcm900/ods_dumble_clean.nam"),
    ("marshall_jcm_900_higain", "Marshall JCM 900 higain", "amps/marshall_jcm900/marshall_jcm_900_higain.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("vox_ac15_crsh_2"),
        &[
            ("vox_ac15_crsh_2", "Vox AC15 Crsh 2"),
            ("jtm_45_crsh", "JTM 45 CRSH"),
            ("vox_ac15_crunsh", "Vox AC15 Crunsh"),
            ("vox_ac15_clean", "Vox AC15 Clean"),
            ("marshall_jtm_45_clean", "Marshall JTM 45 Clean"),
            ("ods_dumble_clean", "ODS Dumble clean"),
            ("marshall_jcm_900_higain", "Marshall JCM 900 higain"),
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
