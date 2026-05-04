use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_tone_king_imperial_preamp";
pub const DISPLAY_NAME: &str = "Imperial Preamp";
const BRAND: &str = "tone_king";

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

// 2-axis preset pack: channel (clean/lead) × gain step (3/5/7/9). Full 4x2 grid.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (channel, gain, _, file)
    ("clean", "3", "", "preamp/tone_king_imperial_preamp/ap_01_imperial_clean_g3.nam"),
    ("clean", "5", "", "preamp/tone_king_imperial_preamp/ap_02_imperial_clean_g5.nam"),
    ("clean", "7", "", "preamp/tone_king_imperial_preamp/ap_03_imperial_clean_g7.nam"),
    ("clean", "9", "", "preamp/tone_king_imperial_preamp/ap_04_imperial_clean_g9.nam"),
    ("lead",  "3", "", "preamp/tone_king_imperial_preamp/ap_05_imperial_lead_g3.nam"),
    ("lead",  "5", "", "preamp/tone_king_imperial_preamp/ap_06_imperial_lead_g5.nam"),
    ("lead",  "7", "", "preamp/tone_king_imperial_preamp/ap_07_imperial_lead_g7.nam"),
    ("lead",  "9", "", "preamp/tone_king_imperial_preamp/ap_08_imperial_lead_g9.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema =
        model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("clean"),
            &[
                ("clean", "Clean"),
                ("lead",  "Lead"),
            ],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
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
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    let model_path = nam::resolve_nam_capture(path)?;
    build_processor_with_assets_for_layout(&model_path, None, plugin_params, sample_rate, layout)
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let channel = required_string(params, "channel").map_err(anyhow::Error::msg)?;
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(c, g, _, _)| *c == channel && *g == gain)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| anyhow!(
            "preamp '{}' has no capture for channel='{}' gain='{}'",
            MODEL_ID, channel, gain
        ))
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
