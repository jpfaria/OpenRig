use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_jmp_2203";
pub const DISPLAY_NAME: &str = "JMP 2203";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("slammin_marshall_2203_scooped_mv4_48k_st", "SLAMMIN_MARSHALL_2203_SCOOPED_MV4_48K_STANDARD", "amps/marshall_jmp_2203/slammin_marshall_2203_scooped_mv4_48k_standard_2.nam"),
    ("slammin_marshall_2203_dark_mv5_48k_stand", "SLAMMIN_MARSHALL_2203_DARK_MV5_48K_STANDARD", "amps/marshall_jmp_2203/slammin_marshall_2203_dark_mv5_48k_standard_2.nam"),
    ("slammin_marshall_2203_noon_mv3_48k_stand", "SLAMMIN_MARSHALL_2203_NOON_MV3_48K_STANDARD", "amps/marshall_jmp_2203/slammin_marshall_2203_noon_mv3_48k_standard_2.nam"),
    ("slammin_marshall_2203_rock_mv7_48k", "SLAMMIN_MARSHALL_2203_ROCK_MV7_48K", "amps/marshall_jmp_2203/slammin_marshall_2203_rock_mv7_48k_2.nam"),
    ("slammin_marshall_2203_wylde_sd1_mv6_48k", "SLAMMIN_MARSHALL_2203_WYLDE_SD1_MV6_48K", "amps/marshall_jmp_2203/slammin_marshall_2203_wylde_sd1_mv6_48k_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("slammin_marshall_2203_scooped_mv4_48k_st"),
        &[
            ("slammin_marshall_2203_scooped_mv4_48k_st", "SLAMMIN_MARSHALL_2203_SCOOPED_MV4_48K_STANDARD"),
            ("slammin_marshall_2203_dark_mv5_48k_stand", "SLAMMIN_MARSHALL_2203_DARK_MV5_48K_STANDARD"),
            ("slammin_marshall_2203_noon_mv3_48k_stand", "SLAMMIN_MARSHALL_2203_NOON_MV3_48K_STANDARD"),
            ("slammin_marshall_2203_rock_mv7_48k", "SLAMMIN_MARSHALL_2203_ROCK_MV7_48K"),
            ("slammin_marshall_2203_wylde_sd1_mv6_48k", "SLAMMIN_MARSHALL_2203_WYLDE_SD1_MV6_48K"),
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
