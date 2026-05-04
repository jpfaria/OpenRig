use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_synergy_syn1";
pub const DISPLAY_NAME: &str = "SYN-1 Chassis";
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

// Single-axis preset pack: 4 K.Kali Syn1 voicings (no clean cartesian).
// Keys/labels cleaned (was "Syn20IR - K.Kali - ..." prefix).
const CAPTURES: &[(&str, &str, &str)] = &[
    ("red",    "Red",    "preamp/synergy_syn1/syn20ir_k_kali_red.nam"),
    ("red_v2", "Red v2", "preamp/synergy_syn1/syn20ir_k_kali_red_v2.nam"),
    ("loomis", "Loomis", "preamp/synergy_syn1/syn20ir_k_kali_loomis.nam"),
    ("green",  "Green",  "preamp/synergy_syn1/syn20ir_k_kali_green.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema =
        model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("red"),
        &[
            ("red",    "Red"),
            ("red_v2", "Red v2"),
            ("loomis", "Loomis"),
            ("green",  "Green"),
        ],
    )];
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
    let key = required_string(params, "preset").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("preamp '{}' has no preset '{}'", MODEL_ID, key))
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
