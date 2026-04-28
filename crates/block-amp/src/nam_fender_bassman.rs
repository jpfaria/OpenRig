use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fender_bassman";
pub const DISPLAY_NAME: &str = "Bassman";
const BRAND: &str = "fender";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("normal_channel_bright_off_g1", "Fender Bassman 50 - Normal Channel - Bright Off - G1", "amps/fender_bassman/fender_bassman_50_normal_channel_bright_off_g1.nam"),
    ("normal_channel_bright_off_g2_5", "Fender Bassman 50 - Normal Channel - Bright Off - G2.5", "amps/fender_bassman/fender_bassman_50_normal_channel_bright_off_g2_5.nam"),
    ("jumped_d0_b1_g5", "FENDER BASSMAN 50 - JUMPED - D0 - B1 - G5", "amps/fender_bassman/fender_bassman_50_jumped_d0_b1_g5.nam"),
    ("jumped_d0_b1_g5_5", "FENDER BASSMAN 50 - JUMPED - D0 - B1 - G5.5", "amps/fender_bassman/fender_bassman_50_jumped_d0_b1_g5_5.nam"),
    ("jumped_d1_b1_g2_5", "FENDER BASSMAN 50 - JUMPED - D1 - B1 - G2.5", "amps/fender_bassman/fender_bassman_50_jumped_d1_b1_g2_5.nam"),
    ("jumped_do_bo_g3", "FENDER BASSMAN 50 - JUMPED - DO - BO - G3", "amps/fender_bassman/fender_bassman_50_jumped_do_bo_g3.nam"),
    ("jumped_d1_b1_g9_5", "FENDER BASSMAN 50 - JUMPED - D1 - B1 - G9.5", "amps/fender_bassman/fender_bassman_50_jumped_d1_b1_g9_5.nam"),
    ("bass_channel_deep_off_g1", "Fender Bassman 50 - Bass Channel - Deep Off - G1", "amps/fender_bassman/fender_bassman_50_bass_channel_deep_off_g1.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("normal_channel_bright_off_g1"),
        &[
            ("normal_channel_bright_off_g1", "Fender Bassman 50 - Normal Channel - Bright Off - G1"),
            ("normal_channel_bright_off_g2_5", "Fender Bassman 50 - Normal Channel - Bright Off - G2.5"),
            ("jumped_d0_b1_g5", "FENDER BASSMAN 50 - JUMPED - D0 - B1 - G5"),
            ("jumped_d0_b1_g5_5", "FENDER BASSMAN 50 - JUMPED - D0 - B1 - G5.5"),
            ("jumped_d1_b1_g2_5", "FENDER BASSMAN 50 - JUMPED - D1 - B1 - G2.5"),
            ("jumped_do_bo_g3", "FENDER BASSMAN 50 - JUMPED - DO - BO - G3"),
            ("jumped_d1_b1_g9_5", "FENDER BASSMAN 50 - JUMPED - D1 - B1 - G9.5"),
            ("bass_channel_deep_off_g1", "Fender Bassman 50 - Bass Channel - Deep Off - G1"),
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
