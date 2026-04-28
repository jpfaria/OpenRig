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

const CAPTURES: &[(&str, &str, &str)] = &[
    ("uberschall_capture_5_emil_settin", "Uberschall Capture 5 Emil settings G6 - Emil Rohbe", "amps/bogner_uberschall/uberschall_capture_5_emil_settings_g6_emil_rohbe.nam"),
    ("uberschall_capture_8_emil_settin", "Uberschall Capture 8 Emil Settings Hi P - Emil Rohbe", "amps/bogner_uberschall/uberschall_capture_8_emil_settings_hi_p_emil_rohbe.nam"),
    ("uberschall_capture_2_g7_emil_roh", "Uberschall Capture 2 G7 - Emil Rohbe", "amps/bogner_uberschall/uberschall_capture_2_g7_emil_rohbe.nam"),
    ("uberschall_capture_9_g6_boosted_", "Uberschall Capture 9 G6 Boosted - Emil Rohbe", "amps/bogner_uberschall/uberschall_capture_9_g6_boosted_emil_rohbe.nam"),
    ("uberschall_capture_13_new_settin", "Uberschall Capture 13 New Settings 2 - Emil Rohbe", "amps/bogner_uberschall/uberschall_capture_13_new_settings_2_emil_rohbe.nam"),
    ("uberschall_capture_14_new_settin", "Uberschall Capture 14 New Settings 3 - Emil Rohbe", "amps/bogner_uberschall/uberschall_capture_14_new_settings_3_emil_rohbe.nam"),
    ("uberschall_capture_7_emil_settin", "Uberschall Capture 7 Emil Settings Lo - Emil Rohbe", "amps/bogner_uberschall/uberschall_capture_7_emil_settings_lo_emil_rohbe.nam"),
    ("uberschall_capture_4_emil_rohbe", "Uberschall Capture 4 - Emil Rohbe", "amps/bogner_uberschall/uberschall_capture_4_emil_rohbe.nam"),
    ("uberschall_capture_17_clean_2_em", "Uberschall Capture 17 Clean 2 - Emil Rohbe", "amps/bogner_uberschall/uberschall_capture_17_clean_2_emil_rohbe.nam"),
    ("uberschall_capture_15_new_settin", "Uberschall Capture 15 New Settings Boosted - Emil Rohbe", "amps/bogner_uberschall/uberschall_capture_15_new_settings_boosted_emil_rohbe.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("uberschall_capture_5_emil_settin"),
        &[
            ("uberschall_capture_5_emil_settin", "Uberschall Capture 5 Emil settings G6 - Emil Rohbe"),
            ("uberschall_capture_8_emil_settin", "Uberschall Capture 8 Emil Settings Hi P - Emil Rohbe"),
            ("uberschall_capture_2_g7_emil_roh", "Uberschall Capture 2 G7 - Emil Rohbe"),
            ("uberschall_capture_9_g6_boosted_", "Uberschall Capture 9 G6 Boosted - Emil Rohbe"),
            ("uberschall_capture_13_new_settin", "Uberschall Capture 13 New Settings 2 - Emil Rohbe"),
            ("uberschall_capture_14_new_settin", "Uberschall Capture 14 New Settings 3 - Emil Rohbe"),
            ("uberschall_capture_7_emil_settin", "Uberschall Capture 7 Emil Settings Lo - Emil Rohbe"),
            ("uberschall_capture_4_emil_rohbe", "Uberschall Capture 4 - Emil Rohbe"),
            ("uberschall_capture_17_clean_2_em", "Uberschall Capture 17 Clean 2 - Emil Rohbe"),
            ("uberschall_capture_15_new_settin", "Uberschall Capture 15 New Settings Boosted - Emil Rohbe"),
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
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no capture '{}'", MODEL_ID, key))
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
