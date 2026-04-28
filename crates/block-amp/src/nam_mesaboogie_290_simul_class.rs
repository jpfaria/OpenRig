use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesaboogie_290_simul_class";
pub const DISPLAY_NAME: &str = "MesaBoogie 290 Simul-Class";
const BRAND: &str = "mesaboogie";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("slammin_mb_290_6l6gc_8_5", "SLAMMIN_MB_290_6L6GC_8_5", "amps/mesaboogie_290_simul_class/slammin_mb_290_6l6gc_8_5.nam"),
    ("slammin_mb_290_6l6gc_8_2", "SLAMMIN_MB_290_6L6GC_8_2", "amps/mesaboogie_290_simul_class/slammin_mb_290_6l6gc_8_2.nam"),
    ("slammin_mb_290_6l6gc_d_6_0", "SLAMMIN_MB_290_6L6GC_D_6_0", "amps/mesaboogie_290_simul_class/slammin_mb_290_6l6gc_d_6_0.nam"),
    ("slammin_mb_290_6l6gc_13_8", "SLAMMIN_MB_290_6L6GC_13_8", "amps/mesaboogie_290_simul_class/slammin_mb_290_6l6gc_13_8.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("slammin_mb_290_6l6gc_8_5"),
        &[
            ("slammin_mb_290_6l6gc_8_5", "SLAMMIN_MB_290_6L6GC_8_5"),
            ("slammin_mb_290_6l6gc_8_2", "SLAMMIN_MB_290_6L6GC_8_2"),
            ("slammin_mb_290_6l6gc_d_6_0", "SLAMMIN_MB_290_6L6GC_D_6_0"),
            ("slammin_mb_290_6l6gc_13_8", "SLAMMIN_MB_290_6L6GC_13_8"),
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
    let key = required_string(params, "preset").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no preset '{}'", MODEL_ID, key))
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
