use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fender_bassman";
pub const DISPLAY_NAME: &str = "Bassman";
const BRAND: &str = "fender";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: channel × gain.
// "Jumped" mode bridges Normal+Bass channels; gain values are knob settings.
// Holes return Err so the UI shows the exact missing combination.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (channel, gain, file)
    ("normal", "g1",  "amps/fender_bassman/fender_bassman_50_normal_channel_bright_off_g1.nam"),
    ("normal", "g2_5","amps/fender_bassman/fender_bassman_50_normal_channel_bright_off_g2_5.nam"),
    ("bass",   "g1",  "amps/fender_bassman/fender_bassman_50_bass_channel_deep_off_g1.nam"),
    ("jumped", "g2_5","amps/fender_bassman/fender_bassman_50_jumped_d1_b1_g2_5.nam"),
    ("jumped", "g3",  "amps/fender_bassman/fender_bassman_50_jumped_do_bo_g3.nam"),
    ("jumped", "g5",  "amps/fender_bassman/fender_bassman_50_jumped_d0_b1_g5.nam"),
    ("jumped", "g5_5","amps/fender_bassman/fender_bassman_50_jumped_d0_b1_g5_5.nam"),
    ("jumped", "g9_5","amps/fender_bassman/fender_bassman_50_jumped_d1_b1_g9_5.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("normal"),
            &[
                ("normal", "Normal"),
                ("bass",   "Bass"),
                ("jumped", "Jumped"),
            ],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some("g1"),
            &[
                ("g1",   "G1"),
                ("g2_5", "G2.5"),
                ("g3",   "G3"),
                ("g5",   "G5"),
                ("g5_5", "G5.5"),
                ("g9_5", "G9.5"),
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
