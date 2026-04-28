use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_randall_rg100es";
pub const DISPLAY_NAME: &str = "Randall RG100es";
const BRAND: &str = "randall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Three-axis pack: channel × gain × sustain. Sparse — only 5 of the 2×2×2 = 8
// combinations were captured. resolve_capture rejects holes.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (channel, gain, sustain, file)
    ("clean",  "default", "off", "amps/randall_rg100es/randall_rg100es_100w_1987_ch_clean.nam"),
    ("crunch", "default", "off", "amps/randall_rg100es/randall_rg100es_100w_1987_ch_crunch.nam"),
    ("crunch", "default", "on",  "amps/randall_rg100es/randall_rg100es_100w_1987_ch_crunch_sustain_engaged.nam"),
    ("crunch", "8_5",     "off", "amps/randall_rg100es/randall_rg100es_100w_1987_ch_crunch_gain_8_5.nam"),
    ("crunch", "8_5",     "on",  "amps/randall_rg100es/randall_rg100es_100w_1987_ch_crunch_sustain_engaged_gain_8_5.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("clean"),
            &[
                ("clean",  "Clean"),
                ("crunch", "Crunch"),
            ],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some("default"),
            &[
                ("default", "Default"),
                ("8_5",     "8.5"),
            ],
        ),
        enum_parameter(
            "sustain",
            "Sustain",
            Some("Amp"),
            Some("off"),
            &[
                ("off", "Off"),
                ("on",  "On"),
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
    let channel = required_string(params, "channel").map_err(anyhow::Error::msg)?;
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    let sustain = required_string(params, "sustain").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(c, g, s, _)| *c == channel && *g == gain && *s == sustain)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for channel={} gain={} sustain={}",
                MODEL_ID, channel, gain, sustain
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
