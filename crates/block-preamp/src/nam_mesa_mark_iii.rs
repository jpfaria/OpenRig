use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_mark_iii";
pub const DISPLAY_NAME: &str = "Mark III";
const BRAND: &str = "mesa";

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

/// (eq, gain, filename)
const CAPTURES: &[(&str, &str, &str)] = &[
    ("lohi",  "cranked", "preamp/nam_mesa_mark_iii/lohi_cranked.nam"),
    ("lohi",  "pushed",  "preamp/nam_mesa_mark_iii/lohi_pushed.nam"),
    ("lomed", "cranked", "preamp/nam_mesa_mark_iii/lomed_cranked.nam"),
    ("lomed", "pushed",  "preamp/nam_mesa_mark_iii/lomed_pushed.nam"),
    ("medhi", "cranked", "preamp/nam_mesa_mark_iii/medhi_cranked.nam"),
    ("medhi", "pushed",  "preamp/nam_mesa_mark_iii/medhi_pushed.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter("eq", "EQ", Some("Amp"), Some("lomed"),
            &[("lohi", "Lo-Hi"), ("lomed", "Lo-Med"), ("medhi", "Med-Hi")]),
        enum_parameter("gain", "Gain", Some("Amp"), Some("pushed"),
            &[("cranked", "Cranked"), ("pushed", "Pushed")]),
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
    let eq = required_string(params, "eq").map_err(anyhow::Error::msg)?;
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    CAPTURES.iter()
        .find(|(e, g, _)| *e == eq && *g == gain)
        .map(|(_, _, f)| *f)
        .ok_or_else(|| anyhow!("unknown eq='{}' gain='{}' for '{}'", eq, gain, MODEL_ID))
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
