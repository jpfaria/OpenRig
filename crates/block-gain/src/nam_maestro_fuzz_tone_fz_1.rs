use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_maestro_fuzz_tone_fz_1";
pub const DISPLAY_NAME: &str = "Maestro Fuzz-Tone FZ-1";
const BRAND: &str = "maestro";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: era × gain.
// 5 captures with one hole at (era=modern, gain=low); resolve_capture
// rejects that combination so both knobs stay independent in the UI.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (era, gain, file)
    ("vintage", "low",  "pedals/maestro_fuzz_tone_fz_1/maestro_fz_m_vintage_low_gain.nam"),
    ("vintage", "mid",  "pedals/maestro_fuzz_tone_fz_1/maestro_fz_m_vintage_mid_gain.nam"),
    ("vintage", "high", "pedals/maestro_fuzz_tone_fz_1/maestro_fz_m_vintage_high_gain.nam"),
    ("modern",  "mid",  "pedals/maestro_fuzz_tone_fz_1/maestro_fz_m_modern_mid_gain.nam"),
    ("modern",  "high", "pedals/maestro_fuzz_tone_fz_1/maestro_fz_m_modern_high_gain.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "era",
            "Era",
            Some("Pedal"),
            Some("vintage"),
            &[
                ("vintage", "Vintage (FZ-1)"),
                ("modern",  "Modern (FZ-M)"),
            ],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Pedal"),
            Some("mid"),
            &[
                ("low",  "Low"),
                ("mid",  "Mid"),
                ("high", "High"),
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

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let era = required_string(params, "era").map_err(anyhow::Error::msg)?;
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(e, g, _)| *e == era && *g == gain)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for era={} gain={}",
                MODEL_ID, era, gain
            )
        })
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
