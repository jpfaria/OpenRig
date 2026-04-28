use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_earthquaker_hizumitas";
pub const DISPLAY_NAME: &str = "EarthQuaker Hizumitas";
const BRAND: &str = "earthquaker";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: sustain × tone (volume fixed at 5).
// 24 captures span the 5×5 grid except (sustain=0, tone=3) which is a hole;
// `resolve_capture` rejects that combination so the UI can keep both knobs free.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (sustain, tone, file)
    ("0",  "0",  "pedals/earthquaker_hizumitas/1_hizumitas_vol_5_sustain_0_tone_0.nam"),
    ("3",  "0",  "pedals/earthquaker_hizumitas/2_hizumitas_vol_5_sustain_3_tone_0.nam"),
    ("5",  "0",  "pedals/earthquaker_hizumitas/3_hizumitas_vol_5_sustain_5_tone_0.nam"),
    ("7",  "0",  "pedals/earthquaker_hizumitas/4_hizumitas_vol_5_sustain_7_tone_0.nam"),
    ("10", "0",  "pedals/earthquaker_hizumitas/5_hizumitas_vol_5_sustain_10_tone_0.nam"),
    ("3",  "3",  "pedals/earthquaker_hizumitas/6_hizumitas_vol_5_sustain_3_tone_3.nam"),
    ("5",  "3",  "pedals/earthquaker_hizumitas/7_hizumitas_vol_5_sustain_5_tone_3.nam"),
    ("7",  "3",  "pedals/earthquaker_hizumitas/8_hizumitas_vol_5_sustain_7_tone_3.nam"),
    ("10", "3",  "pedals/earthquaker_hizumitas/9_hizumitas_vol_5_sustain_10_tone_3.nam"),
    ("0",  "5",  "pedals/earthquaker_hizumitas/10_hizumitas_vol_5_sustain_0_tone_5.nam"),
    ("3",  "5",  "pedals/earthquaker_hizumitas/11_hizumitas_vol_5_sustain_3_tone_5.nam"),
    ("5",  "5",  "pedals/earthquaker_hizumitas/12_hizumitas_vol_5_sustain_5_tone_5.nam"),
    ("7",  "5",  "pedals/earthquaker_hizumitas/13_hizumitas_vol_5_sustain_7_tone_5.nam"),
    ("10", "5",  "pedals/earthquaker_hizumitas/14_hizumitas_vol_5_sustain_10_tone_5.nam"),
    ("0",  "7",  "pedals/earthquaker_hizumitas/15_hizumitas_vol_5_sustain_0_tone_7.nam"),
    ("3",  "7",  "pedals/earthquaker_hizumitas/16_hizumitas_vol_5_sustain_3_tone_7.nam"),
    ("5",  "7",  "pedals/earthquaker_hizumitas/17_hizumitas_vol_5_sustain_5_tone_7.nam"),
    ("7",  "7",  "pedals/earthquaker_hizumitas/18_hizumitas_vol_5_sustain_7_tone_7.nam"),
    ("10", "7",  "pedals/earthquaker_hizumitas/19_hizumitas_vol_5_sustain_10_tone_7.nam"),
    ("0",  "10", "pedals/earthquaker_hizumitas/20_hizumitas_vol_5_sustain_0_tone_10.nam"),
    ("3",  "10", "pedals/earthquaker_hizumitas/21_hizumitas_vol_5_sustain_3_tone_10.nam"),
    ("5",  "10", "pedals/earthquaker_hizumitas/22_hizumitas_vol_5_sustain_5_tone_10.nam"),
    ("7",  "10", "pedals/earthquaker_hizumitas/23_hizumitas_vol_5_sustain_7_tone_10.nam"),
    ("10", "10", "pedals/earthquaker_hizumitas/24_hizumitas_vol_5_sustain_10_tone_10.nam"),
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
                ("0",  "0"),
                ("3",  "3"),
                ("5",  "5"),
                ("7",  "7"),
                ("10", "10"),
            ],
        ),
        enum_parameter(
            "tone",
            "Tone",
            Some("Pedal"),
            Some("5"),
            &[
                ("0",  "0"),
                ("3",  "3"),
                ("5",  "5"),
                ("7",  "7"),
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
