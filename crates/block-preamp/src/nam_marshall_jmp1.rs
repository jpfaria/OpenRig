use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_jmp1";
pub const DISPLAY_NAME: &str = "JMP-1";
const BRAND: &str = "marshall";

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

// Single-axis preset pack: 8 voicings spanning Clean / Crunch / Rhythm 1-3 +
// boosted variants / Lead. Variants per category are irregular — does not
// decompose cleanly. Keys/labels cleaned (was "JMP ..." uppercase prefix).
const CAPTURES: &[(&str, &str, &str)] = &[
    ("clean",            "Clean",             "preamp/marshall_jmp1/jmp_clean.nam"),
    ("crunch_breakup",   "Crunch Breakup",    "preamp/marshall_jmp1/jmp_crunch_breakup.nam"),
    ("rhythm_1",         "Rhythm 1",          "preamp/marshall_jmp1/jmp_rhythm_1.nam"),
    ("rhythm_2",         "Rhythm 2",          "preamp/marshall_jmp1/jmp_rhythm_2.nam"),
    ("rhythm_3",         "Rhythm 3",          "preamp/marshall_jmp1/jmp_rhythm_3.nam"),
    ("rhythm_deftoned",  "Rhythm Deftoned",   "preamp/marshall_jmp1/jmp_rhythm_deftoned.nam"),
    ("rhythm_od808",     "Rhythm OD808",      "preamp/marshall_jmp1/jmp_rhythm_overdrive808.nam"),
    ("lead_creamy",      "Lead Creamy",       "preamp/marshall_jmp1/jmp_lead_creamy.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema =
        model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("clean"),
        &[
            ("clean",            "Clean"),
            ("crunch_breakup",   "Crunch Breakup"),
            ("rhythm_1",         "Rhythm 1"),
            ("rhythm_2",         "Rhythm 2"),
            ("rhythm_3",         "Rhythm 3"),
            ("rhythm_deftoned",  "Rhythm Deftoned"),
            ("rhythm_od808",     "Rhythm OD808"),
            ("lead_creamy",      "Lead Creamy"),
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
