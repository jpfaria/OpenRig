use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_jet_city_jca22h";
pub const DISPLAY_NAME: &str = "Jet City - JCA22H";
const BRAND: &str = "jet";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: channel × gain step.
// Only 5 of the 2×5 = 10 possible combinations were captured. The
// `resolve_capture` lookup rejects the holes so the UI exposes both
// knobs as independent controls.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (channel, gain, file)
    ("crunch",    "g2_0", "amps/jet_city_jca22h/jet_city_jca22h_crunch_g2_0.nam"),
    ("crunch",    "g4_5", "amps/jet_city_jca22h/jet_city_jca22h_crunch_g4_5.nam"),
    ("crunch",    "g7_0", "amps/jet_city_jca22h/jet_city_jca22h_crunch_g7_0.nam"),
    ("overdrive", "g2_0", "amps/jet_city_jca22h/jet_city_jca22h_overdrive_g2_0.nam"),
    ("overdrive", "g5_5", "amps/jet_city_jca22h/jet_city_jca22h_overdrive_g5_5.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("crunch"),
            &[
                ("crunch",    "Crunch"),
                ("overdrive", "Overdrive"),
            ],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some("g4_5"),
            &[
                ("g2_0", "G2.0"),
                ("g4_5", "G4.5"),
                ("g5_5", "G5.5"),
                ("g7_0", "G7.0"),
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
    CAPTURES
        .iter()
        .find(|(c, g, _)| *c == channel && *g == gain)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for channel={} gain={}",
                MODEL_ID, channel, gain
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
