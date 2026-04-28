use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_engl_e530_preamp";
pub const DISPLAY_NAME: &str = "E530 Preamp";
const BRAND: &str = "engl";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Single-axis: 6 voicings — Channel 1 (clean) and 2 (modern), DI vs CTR'd,
// optional TS1 boost on Modern.
const CAPTURES: &[(&str, &str, &str)] = &[
    ("ch1",            "Ch1",             "amps/engl_e530_preamp/e530_1.nam"),
    ("ch1_ctr",        "Ch1 CTR",         "amps/engl_e530_preamp/e530_1_di_ctr.nam"),
    ("ch2_di",         "Ch2 DI",          "amps/engl_e530_preamp/e530_2_di.nam"),
    ("ch2_ctr",        "Ch2 CTR",         "amps/engl_e530_preamp/e530_2_di_ctr.nam"),
    ("modern",         "Modern",          "amps/engl_e530_preamp/engl_e530_modern.nam"),
    ("modern_ts1",     "Modern + TS1",    "amps/engl_e530_preamp/engl_e530_modern_ts1.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("ch1"),
        &[
            ("ch1",        "Ch1"),
            ("ch1_ctr",    "Ch1 CTR"),
            ("ch2_di",     "Ch2 DI"),
            ("ch2_ctr",    "Ch2 CTR"),
            ("modern",     "Modern"),
            ("modern_ts1", "Modern + TS1"),
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
