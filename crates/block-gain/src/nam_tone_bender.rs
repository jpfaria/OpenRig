use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_tone_bender";
pub const DISPLAY_NAME: &str = "Tone Bender";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: preset × voltage (Boss TB-2W reissue).
// 5 nominal preset positions × 3 supply voltages = 15 captures, full grid.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (preset, voltage, file)
    ("01", "7v",  "pedals/tone_bender/boss_tb_2w_01_7v.nam"),
    ("01", "9v",  "pedals/tone_bender/boss_tb_2w_01_9v.nam"),
    ("01", "12v", "pedals/tone_bender/boss_tb_2w_01_12v.nam"),
    ("02", "7v",  "pedals/tone_bender/boss_tb_2w_02_7v.nam"),
    ("02", "9v",  "pedals/tone_bender/boss_tb_2w_02_9v.nam"),
    ("02", "12v", "pedals/tone_bender/boss_tb_2w_02_12v.nam"),
    ("03", "7v",  "pedals/tone_bender/boss_tb_2w_03_7v.nam"),
    ("03", "9v",  "pedals/tone_bender/boss_tb_2w_03_9v.nam"),
    ("03", "12v", "pedals/tone_bender/boss_tb_2w_03_12v.nam"),
    ("04", "7v",  "pedals/tone_bender/boss_tb_2w_04_7v.nam"),
    ("04", "9v",  "pedals/tone_bender/boss_tb_2w_04_9v.nam"),
    ("04", "12v", "pedals/tone_bender/boss_tb_2w_04_12v.nam"),
    ("05", "7v",  "pedals/tone_bender/boss_tb_2w_05_7v.nam"),
    ("05", "9v",  "pedals/tone_bender/boss_tb_2w_05_9v.nam"),
    ("05", "12v", "pedals/tone_bender/boss_tb_2w_05_12v.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "preset",
            "Preset",
            Some("Pedal"),
            Some("03"),
            &[
                ("01", "Preset 1"),
                ("02", "Preset 2"),
                ("03", "Preset 3"),
                ("04", "Preset 4"),
                ("05", "Preset 5"),
            ],
        ),
        enum_parameter(
            "voltage",
            "Sag (Supply V)",
            Some("Pedal"),
            Some("9v"),
            &[
                ("7v",  "7 V"),
                ("9v",  "9 V"),
                ("12v", "12 V"),
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
    let preset = required_string(params, "preset").map_err(anyhow::Error::msg)?;
    let voltage = required_string(params, "voltage").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(p, v, _)| *p == preset && *v == voltage)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for preset={} voltage={}",
                MODEL_ID, preset, voltage
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
