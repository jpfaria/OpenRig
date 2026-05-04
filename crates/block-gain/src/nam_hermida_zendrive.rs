use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_hermida_zendrive";
pub const DISPLAY_NAME: &str = "Hermida Zendrive";
const BRAND: &str = "hermida";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voice × gain (vol fixed at 100%, tone at 1:30).
// Clock-position knobs: 130 ≈ low, 1030 ≈ high, 1200 = max.
// 21 captures cover most combinations; the (voice=1:30, gain=1:30) and
// (voice ≠ 1:30, gain = 1:00) pairs are holes — `resolve_capture` rejects them.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voice, gain, file)
    ("130",  "300",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain300.nam"),
    ("130",  "500",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain500.nam"),
    ("130",  "700",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain700.nam"),
    ("130",  "900",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain900.nam"),
    ("130",  "1030", "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain1030.nam"),
    ("130",  "1200", "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain1200.nam"),
    ("130",  "1300", "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain1300.nam"),
    ("1030", "130",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain130.nam"),
    ("1030", "300",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain300.nam"),
    ("1030", "500",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain500.nam"),
    ("1030", "700",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain700.nam"),
    ("1030", "900",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain900.nam"),
    ("1030", "1030", "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain1030.nam"),
    ("1030", "1200", "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain1200.nam"),
    ("1200", "130",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain130.nam"),
    ("1200", "300",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain300.nam"),
    ("1200", "500",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain500.nam"),
    ("1200", "700",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain700.nam"),
    ("1200", "900",  "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain900.nam"),
    ("1200", "1030", "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain1030.nam"),
    ("1200", "1200", "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain1200.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voice",
            "Voice",
            Some("Pedal"),
            Some("1030"),
            &[
                ("130",  "1:30"),
                ("1030", "10:30"),
                ("1200", "12:00"),
            ],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Pedal"),
            Some("700"),
            &[
                ("130",  "1:30"),
                ("300",  "3:00"),
                ("500",  "5:00"),
                ("700",  "7:00"),
                ("900",  "9:00"),
                ("1030", "10:30"),
                ("1200", "12:00"),
                ("1300", "1:00"),
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
    let voice = required_string(params, "voice").map_err(anyhow::Error::msg)?;
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, g, _)| *v == voice && *g == gain)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for voice={} gain={}",
                MODEL_ID, voice, gain
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
