use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_digitech_grunge";
pub const DISPLAY_NAME: &str = "DigiTech Grunge";
const BRAND: &str = "digitech";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: tone × grunge.
// 2 × 4 = 8 captures, full grid.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (tone, grunge, file)
    ("5", "3", "pedals/digitech_grunge/digitech_grunge_tone_5_grunge_3.nam"),
    ("5", "5", "pedals/digitech_grunge/digitech_grunge_tone_5_grunge_5.nam"),
    ("5", "7", "pedals/digitech_grunge/digitech_grunge_tone_5_grunge_7.nam"),
    ("5", "9", "pedals/digitech_grunge/digitech_grunge_tone_5_grunge_9.nam"),
    ("7", "3", "pedals/digitech_grunge/digitech_grunge_tone_7_grunge_3.nam"),
    ("7", "5", "pedals/digitech_grunge/digitech_grunge_tone_7_grunge_5.nam"),
    ("7", "7", "pedals/digitech_grunge/digitech_grunge_tone_7_grunge_7.nam"),
    ("7", "9", "pedals/digitech_grunge/digitech_grunge_tone_7_grunge_9.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "tone",
            "Tone",
            Some("Pedal"),
            Some("5"),
            &[
                ("5", "5"),
                ("7", "7"),
            ],
        ),
        enum_parameter(
            "grunge",
            "Grunge",
            Some("Pedal"),
            Some("5"),
            &[
                ("3", "3"),
                ("5", "5"),
                ("7", "7"),
                ("9", "9"),
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
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    let grunge = required_string(params, "grunge").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(t, g, _)| *t == tone && *g == grunge)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for tone={} grunge={}",
                MODEL_ID, tone, grunge
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
