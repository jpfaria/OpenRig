use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_synergy_friedman_ds";
pub const DISPLAY_NAME: &str = "Friedman DS Module";
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

// Single-axis preset pack: 4 Friedman DS voicings with sparse, irregular knob
// settings (channel × bright with gain/EQ varying per row — not a clean grid).
// Keys/labels cleaned (was "Syn20IR DS ... r51 M32" prefix).
const CAPTURES: &[(&str, &str, &str)] = &[
    ("ch1_a",      "Ch1 A",      "preamp/synergy_friedman_ds/syn20ir_ds_c1_a_2_g37_b68_m51_t40_v51_r51_m32.nam"),
    ("ch2_b",      "Ch2 B",      "preamp/synergy_friedman_ds/syn20ir_ds_c2_b_2_g37_b68_m51_t40_v45_r51_m32.nam"),
    ("ch2_b_brt",  "Ch2 B Brt",  "preamp/synergy_friedman_ds/syn20ir_ds_c2_b_brt_2_g37_b68_m51_t40_v45_r51_m32.nam"),
    ("ch2_c_brt",  "Ch2 C Brt",  "preamp/synergy_friedman_ds/syn20ir_ds_c2_c_brt_3_g44_b50_m61_t48_v37_r51_m32.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema =
        model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("ch1_a"),
        &[
            ("ch1_a",      "Ch1 A"),
            ("ch2_b",      "Ch2 B"),
            ("ch2_b_brt",  "Ch2 B Brt"),
            ("ch2_c_brt",  "Ch2 C Brt"),
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
