use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_engl_e305_gigmaster_30_head";
pub const DISPLAY_NAME: &str = "E305 Gigmaster 30 Head";
const BRAND: &str = "engl";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Single-axis gain pack: Clean Lead Ch + Gain Boost + TS9 boost, gain 2-11.
const CAPTURES: &[(&str, &str, &str)] = &[
    ("g2",  "Gain 2",  "amps/engl_e305_gigmaster_30_head/e305_clean_lead_ch_gain_boost_ibanez_tube_screamer_gain_2.nam"),
    ("g6",  "Gain 6",  "amps/engl_e305_gigmaster_30_head/e305_clean_lead_ch_gain_boost_ibanez_tube_screamer_gain_6.nam"),
    ("g8",  "Gain 8",  "amps/engl_e305_gigmaster_30_head/e305_clean_lead_ch_gain_boost_ibanez_tube_screamer_gain_8.nam"),
    ("g10", "Gain 10", "amps/engl_e305_gigmaster_30_head/e305_clean_lead_ch_gain_boost_ibanez_tube_screamer_gain_10.nam"),
    ("g11", "Gain 11", "amps/engl_e305_gigmaster_30_head/e305_clean_lead_ch_gain_boost_ibanez_tube_screamer_gain_11.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "gain",
        "Gain",
        Some("Amp"),
        Some("g6"),
        &[
            ("g2",  "Gain 2"),
            ("g6",  "Gain 6"),
            ("g8",  "Gain 8"),
            ("g10", "Gain 10"),
            ("g11", "Gain 11"),
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
    let key = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no gain step '{}'", MODEL_ID, key))
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
