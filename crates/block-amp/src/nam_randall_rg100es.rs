use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_randall_rg100es";
pub const DISPLAY_NAME: &str = "Randall RG100es";
const BRAND: &str = "randall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("clean", "RANDALL RG100ES 100W (1987) CH Clean", "amps/randall_rg100es/randall_rg100es_100w_1987_ch_clean.nam"),
    ("crunch", "RANDALL RG100ES 100W (1987) CH Crunch", "amps/randall_rg100es/randall_rg100es_100w_1987_ch_crunch.nam"),
    ("crunch_sustain_engaged_gain_8_5", "RANDALL RG100ES 100W (1987) CH Crunch Sustain Engaged Gain 8", "amps/randall_rg100es/randall_rg100es_100w_1987_ch_crunch_sustain_engaged_gain_8_5.nam"),
    ("crunch_gain_8_5", "RANDALL RG100ES 100W (1987) CH Crunch Gain 8.5", "amps/randall_rg100es/randall_rg100es_100w_1987_ch_crunch_gain_8_5.nam"),
    ("crunch_sustain_engaged", "RANDALL RG100ES 100W (1987) CH Crunch Sustain Engaged", "amps/randall_rg100es/randall_rg100es_100w_1987_ch_crunch_sustain_engaged.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("clean"),
        &[
            ("clean", "RANDALL RG100ES 100W (1987) CH Clean"),
            ("crunch", "RANDALL RG100ES 100W (1987) CH Crunch"),
            ("crunch_sustain_engaged_gain_8_5", "RANDALL RG100ES 100W (1987) CH Crunch Sustain Engaged Gain 8"),
            ("crunch_gain_8_5", "RANDALL RG100ES 100W (1987) CH Crunch Gain 8.5"),
            ("crunch_sustain_engaged", "RANDALL RG100ES 100W (1987) CH Crunch Sustain Engaged"),
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
