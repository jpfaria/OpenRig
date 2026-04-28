use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_splawn_quickrod";
pub const DISPLAY_NAME: &str = "Quickrod";
const BRAND: &str = "splawn";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Single-axis: 7 voicings on Quickrod's gain channels — varying gain step
// (3/5), mids (3/5), and HG/HGt modes. Sparse — does not decompose cleanly.
const CAPTURES: &[(&str, &str, &str)] = &[
    ("g3_mids3",     "G3 Mids3",     "amps/splawn_quickrod/splawn3g7m_mids_3.nam"),
    ("g3_mids5",     "G3 Mids5",     "amps/splawn_quickrod/splawn3g7m_mids_5.nam"),
    ("g5_mids5",     "G5 Mids5",     "amps/splawn_quickrod/splawn5g7m_mids_5.nam"),
    ("hg_g3_mids5",  "HG G3 Mids5",  "amps/splawn_quickrod/splawn_hg3g7m_mids5.nam"),
    ("hg_g5_mids5",  "HG G5 Mids5",  "amps/splawn_quickrod/splawn_hg5g7m_mids5.nam"),
    ("hgt_g3",       "HGt G3",       "amps/splawn_quickrod/splawnhgt3g7m.nam"),
    ("hgt_g5",       "HGt G5",       "amps/splawn_quickrod/splawnhgt5g7m.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("g3_mids3"),
        &[
            ("g3_mids3",     "G3 Mids3"),
            ("g3_mids5",     "G3 Mids5"),
            ("g5_mids5",     "G5 Mids5"),
            ("hg_g3_mids5",  "HG G3 Mids5"),
            ("hg_g5_mids5",  "HG G5 Mids5"),
            ("hgt_g3",       "HGt G3"),
            ("hgt_g5",       "HGt G5"),
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
