use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_earthquaker_plumes";
pub const DISPLAY_NAME: &str = "EarthQuaker Plumes";
const BRAND: &str = "earthquaker";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: drive × tone (mode switch fixed at "3 / Bright").
// 3 drive points pair gain with a make-up level (low gain → high level, etc.)
// × 5 tone steps = 15 captures, full grid.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (drive, tone, file)
    ("low",  "0",   "pedals/earthquaker_plumes/plumes_switch_3_level_100_gain_0_tone_0.nam"),
    ("low",  "25",  "pedals/earthquaker_plumes/plumes_switch_3_level_100_gain_0_tone_25.nam"),
    ("low",  "50",  "pedals/earthquaker_plumes/plumes_switch_3_level_100_gain_0_tone_50.nam"),
    ("low",  "75",  "pedals/earthquaker_plumes/plumes_switch_3_level_100_gain_0_tone_75.nam"),
    ("low",  "100", "pedals/earthquaker_plumes/plumes_switch_3_level_100_gain_0_tone_100.nam"),
    ("mid",  "0",   "pedals/earthquaker_plumes/plumes_switch_3_level_50_gain_50_tone_0.nam"),
    ("mid",  "25",  "pedals/earthquaker_plumes/plumes_switch_3_level_50_gain_50_tone_25.nam"),
    ("mid",  "50",  "pedals/earthquaker_plumes/plumes_switch_3_level_50_gain_50_tone_50.nam"),
    ("mid",  "75",  "pedals/earthquaker_plumes/plumes_switch_3_level_50_gain_50_tone_75.nam"),
    ("mid",  "100", "pedals/earthquaker_plumes/plumes_switch_3_level_50_gain_50_tone_100.nam"),
    ("high", "0",   "pedals/earthquaker_plumes/plumes_switch_3_level_25_gain_75_tone_0.nam"),
    ("high", "25",  "pedals/earthquaker_plumes/plumes_switch_3_level_25_gain_75_tone_25.nam"),
    ("high", "50",  "pedals/earthquaker_plumes/plumes_switch_3_level_25_gain_75_tone_50.nam"),
    ("high", "75",  "pedals/earthquaker_plumes/plumes_switch_3_level_25_gain_75_tone_75.nam"),
    ("high", "100", "pedals/earthquaker_plumes/plumes_switch_3_level_25_gain_75_tone_100.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "drive",
            "Drive",
            Some("Pedal"),
            Some("mid"),
            &[
                ("low",  "Low (clean)"),
                ("mid",  "Mid"),
                ("high", "High"),
            ],
        ),
        enum_parameter(
            "tone",
            "Tone",
            Some("Pedal"),
            Some("50"),
            &[
                ("0",   "0%"),
                ("25",  "25%"),
                ("50",  "50%"),
                ("75",  "75%"),
                ("100", "100%"),
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
    let drive = required_string(params, "drive").map_err(anyhow::Error::msg)?;
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(d, t, _)| *d == drive && *t == tone)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for drive={} tone={}",
                MODEL_ID, drive, tone
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
