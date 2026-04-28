use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_1959hw";
pub const DISPLAY_NAME: &str = "1959HW";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: gain × tone. All Slammin' Plexi Cranked.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (gain, tone, file)
    ("g6", "complex",  "amps/marshall_1959hw/slammin_plexi_cranked_6_1700e_complex_custom.nam"),
    ("g7", "standard", "amps/marshall_1959hw/slammin_plexi_cranked_7_1500e_standard.nam"),
    ("g7", "standard_2000e", "amps/marshall_1959hw/slammin_plexi_cranked_7_2000e_standard.nam"),
    ("g7", "complex",  "amps/marshall_1959hw/slammin_plexi_cranked_7_2000e_complex_custom.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some("g7"),
            &[
                ("g6", "Cranked 6"),
                ("g7", "Cranked 7"),
            ],
        ),
        enum_parameter(
            "tone",
            "Tone",
            Some("Amp"),
            Some("standard"),
            &[
                ("standard",      "Standard"),
                ("standard_2000e","Standard (2000E)"),
                ("complex",       "Complex"),
            ],
        ),
    ];
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
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(g, t, _)| *g == gain && *t == tone)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for gain={} tone={}",
                MODEL_ID, gain, tone
            )
        })
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
