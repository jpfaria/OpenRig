use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_bogner_uberschall";
pub const DISPLAY_NAME: &str = "Uberschall";
const BRAND: &str = "bogner";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Single-axis preset pack from Emil Rohbe — capture numbers map to amp settings.
// Labels stripped of common "Uberschall Capture N" prefix and " - Emil Rohbe" suffix.
const CAPTURES: &[(&str, &str, &str)] = &[
    ("cap_2_g7",          "Cap 2: G7",            "amps/bogner_uberschall/uberschall_capture_2_g7_emil_rohbe.nam"),
    ("cap_4",             "Cap 4",                "amps/bogner_uberschall/uberschall_capture_4_emil_rohbe.nam"),
    ("cap_5_g6",          "Cap 5: G6",            "amps/bogner_uberschall/uberschall_capture_5_emil_settings_g6_emil_rohbe.nam"),
    ("cap_7_lo",          "Cap 7: Lo",            "amps/bogner_uberschall/uberschall_capture_7_emil_settings_lo_emil_rohbe.nam"),
    ("cap_8_hi_p",        "Cap 8: Hi P",          "amps/bogner_uberschall/uberschall_capture_8_emil_settings_hi_p_emil_rohbe.nam"),
    ("cap_9_g6_boosted",  "Cap 9: G6 Boosted",    "amps/bogner_uberschall/uberschall_capture_9_g6_boosted_emil_rohbe.nam"),
    ("cap_13_new_2",      "Cap 13: New 2",        "amps/bogner_uberschall/uberschall_capture_13_new_settings_2_emil_rohbe.nam"),
    ("cap_14_new_3",      "Cap 14: New 3",        "amps/bogner_uberschall/uberschall_capture_14_new_settings_3_emil_rohbe.nam"),
    ("cap_15_new_boost",  "Cap 15: New Boosted",  "amps/bogner_uberschall/uberschall_capture_15_new_settings_boosted_emil_rohbe.nam"),
    ("cap_17_clean_2",    "Cap 17: Clean 2",      "amps/bogner_uberschall/uberschall_capture_17_clean_2_emil_rohbe.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("cap_5_g6"),
        &[
            ("cap_2_g7",         "Cap 2: G7"),
            ("cap_4",            "Cap 4"),
            ("cap_5_g6",         "Cap 5: G6"),
            ("cap_7_lo",         "Cap 7: Lo"),
            ("cap_8_hi_p",       "Cap 8: Hi P"),
            ("cap_9_g6_boosted", "Cap 9: G6 Boosted"),
            ("cap_13_new_2",     "Cap 13: New 2"),
            ("cap_14_new_3",     "Cap 14: New 3"),
            ("cap_15_new_boost", "Cap 15: New Boosted"),
            ("cap_17_clean_2",   "Cap 17: Clean 2"),
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
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let key = required_string(params, "preset").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no preset '{}'", MODEL_ID, key))
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
