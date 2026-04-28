use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_jcm2000_tsl";
pub const DISPLAY_NAME: &str = "JCM2000 TSL";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("crunch_g9", "TSL100 Crunch - G9", "amps/marshall_jcm2000_tsl/tsl100_crunch_g9_2.nam"),
    ("crunch_g2", "TSL100 Crunch - G2", "amps/marshall_jcm2000_tsl/tsl100_crunch_g2_2.nam"),
    ("lead_g9", "TSL100 Lead - G9", "amps/marshall_jcm2000_tsl/tsl100_lead_g9_2.nam"),
    ("crunch_g6", "TSL100 Crunch - G6", "amps/marshall_jcm2000_tsl/tsl100_crunch_g6_2.nam"),
    ("lead_g7", "TSL100 Lead - G7", "amps/marshall_jcm2000_tsl/tsl100_lead_g7_2.nam"),
    ("clean_g3_5", "TSL100 Clean - G3.5", "amps/marshall_jcm2000_tsl/tsl100_clean_g3_5_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("crunch_g9"),
        &[
            ("crunch_g9", "TSL100 Crunch - G9"),
            ("crunch_g2", "TSL100 Crunch - G2"),
            ("lead_g9", "TSL100 Lead - G9"),
            ("crunch_g6", "TSL100 Crunch - G6"),
            ("lead_g7", "TSL100 Lead - G7"),
            ("clean_g3_5", "TSL100 Clean - G3.5"),
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
