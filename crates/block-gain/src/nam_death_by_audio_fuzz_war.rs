use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_death_by_audio_fuzz_war";
pub const DISPLAY_NAME: &str = "Death By Audio Fuzz War";
const BRAND: &str = "death_by_audio";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: sustain × tone.
// 2 sustain × 3 tone steps = 6 captures, full grid. The original keys
// were truncated by codegen (e.g. `5_tone_5_fuzz_ware_sus5_ton`); this
// rewrite re-derives clean axes directly from the .nam filenames.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (sustain, tone, file)
    ("5",  "1",  "pedals/death_by_audio_fuzz_war/fuzz_war_clone_sustain_5_tone_1_fuzz_war_sus5_ton1.nam"),
    ("5",  "5",  "pedals/death_by_audio_fuzz_war/fuzz_war_clone_sustain_5_tone_5_fuzz_ware_sus5_ton.nam"),
    ("5",  "10", "pedals/death_by_audio_fuzz_war/fuzz_war_clone_sustain_5_tone_10_fuzz_war_sus5_ton.nam"),
    ("10", "1",  "pedals/death_by_audio_fuzz_war/fuzz_war_clone_sustain_10_tone_1_fuzz_war_sus10_to.nam"),
    ("10", "5",  "pedals/death_by_audio_fuzz_war/fuzz_war_clone_sustain_10_tone_5_fuzz_war_sus10_to.nam"),
    ("10", "10", "pedals/death_by_audio_fuzz_war/fuzz_war_clone_sustain_10_tone_10_fuzz_war_sus10_t.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "sustain",
            "Sustain",
            Some("Pedal"),
            Some("5"),
            &[
                ("5",  "5"),
                ("10", "10"),
            ],
        ),
        enum_parameter(
            "tone",
            "Tone",
            Some("Pedal"),
            Some("5"),
            &[
                ("1",  "1"),
                ("5",  "5"),
                ("10", "10"),
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
    let sustain = required_string(params, "sustain").map_err(anyhow::Error::msg)?;
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(s, t, _)| *s == sustain && *t == tone)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for sustain={} tone={}",
                MODEL_ID, sustain, tone
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
