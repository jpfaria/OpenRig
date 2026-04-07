use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fender_57_deluxe";
pub const DISPLAY_NAME: &str = "'57 Custom Deluxe";
const BRAND: &str = "fender";

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

const CAPTURES: &[(&str, &str, &str)] = &[
    ("clean",  "1", "preamp/nam_fender_57_deluxe/clean_in1.nam"),
    ("clean",  "2", "preamp/nam_fender_57_deluxe/clean_in2.nam"),
    ("crunch", "1", "preamp/nam_fender_57_deluxe/crunch_in1.nam"),
    ("crunch", "2", "preamp/nam_fender_57_deluxe/crunch_in2.nam"),
    ("od",     "1", "preamp/nam_fender_57_deluxe/od_in1.nam"),
    ("od",     "2", "preamp/nam_fender_57_deluxe/od_in2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter("voicing", "Voicing", Some("Amp"), Some("clean"),
            &[("clean", "Clean"), ("crunch", "Crunch"), ("od", "OD")]),
        enum_parameter("input", "Input", Some("Amp"), Some("1"),
            &[("1", "Input 1"), ("2", "Input 2")]),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let nam_file = resolve_capture(params)?;
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    let model_path = nam::resolve_nam_capture(nam_file)?;
    build_processor_with_assets_for_layout(&model_path, None, plugin_params, sample_rate, layout)
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let voicing = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    let input = required_string(params, "input").map_err(anyhow::Error::msg)?;
    CAPTURES.iter()
        .find(|(v, i, _)| *v == voicing && *i == input)
        .map(|(_, _, f)| *f)
        .ok_or_else(|| anyhow!("unknown voicing='{}' input='{}' for '{}'", voicing, input, MODEL_ID))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
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
    let file = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", file))
}
