use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_synergy_bogner_ecstasy";
pub const DISPLAY_NAME: &str = "Bogner Ecstasy Module";
const BRAND: &str = "synergy";

pub const NAM_PLUGIN_DEFAULTS: NamPluginParams = NamPluginParams {
    input_level_db: 0.0,
    output_level_db: 0.0,
    noise_gate_threshold_db: -80.0,
    noise_gate_enabled: true,
    eq_enabled: true,
    bass: 5.0,
    middle: 5.0,
    treble: 5.0,
};

// Two-axis pack: channel × position. Sparse — Blue captured at #08 only,
// Red at #03/07/09. resolve_capture rejects missing combinations.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (channel, position, file)
    ("blue", "08", "preamp/synergy_bogner_ecstasy/reacteq_bogner_ecstasy_blue_08.nam"),
    ("red",  "03", "preamp/synergy_bogner_ecstasy/reacteq_bogner_ecstasy_red_03.nam"),
    ("red",  "07", "preamp/synergy_bogner_ecstasy/reacteq_bogner_ecstasy_red_07.nam"),
    ("red",  "09", "preamp/synergy_bogner_ecstasy/reacteq_bogner_ecstasy_red_09.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema =
        model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("red"),
            &[
                ("blue", "Blue"),
                ("red",  "Red"),
            ],
        ),
        enum_parameter(
            "position",
            "Position",
            Some("Amp"),
            Some("07"),
            &[
                ("03", "#03"),
                ("07", "#07"),
                ("08", "#08"),
                ("09", "#09"),
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
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    let model_path = nam::resolve_nam_capture(path)?;
    build_processor_with_assets_for_layout(&model_path, None, plugin_params, sample_rate, layout)
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let channel = required_string(params, "channel").map_err(anyhow::Error::msg)?;
    let position = required_string(params, "position").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(c, p, _)| *c == channel && *p == position)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "preamp '{}' has no capture for channel={} position={}",
                MODEL_ID, channel, position
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

pub const MODEL_DEFINITION: PreampModelDefinition = PreampModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: PreampBackendKind::Nam,
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
    Ok(format!("asset_id='{}'", path))
}
