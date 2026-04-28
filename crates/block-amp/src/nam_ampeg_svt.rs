use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_ampeg_svt";
pub const DISPLAY_NAME: &str = "SVT";
const BRAND: &str = "ampeg";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("ampeg_svt_ultra_hi_md_421", "Ampeg SVT - Ultra Hi MD 421", "amps/ampeg_svt/ampeg_svt_ultra_hi_md_421.nam"),
    ("ampeg_svt_ultra_lo_sm57", "Ampeg SVT - Ultra Lo SM57", "amps/ampeg_svt/ampeg_svt_ultra_lo_sm57.nam"),
    ("ampeg_svt_sm75", "Ampeg SVT - SM75", "amps/ampeg_svt/ampeg_svt_sm75.nam"),
    ("ampeg_svt_ultra_lo_md_421", "Ampeg SVT - Ultra Lo MD 421", "amps/ampeg_svt/ampeg_svt_ultra_lo_md_421.nam"),
    ("ampeg_svt_md_421", "Ampeg SVT - MD 421", "amps/ampeg_svt/ampeg_svt_md_421.nam"),
    ("ampeg_svt_ultra_hi_sm57", "Ampeg SVT - Ultra Hi SM57", "amps/ampeg_svt/ampeg_svt_ultra_hi_sm57.nam"),
    ("ampeg_svt_gain_10_ultra_lo_and_h", "Ampeg SVT - Gain 10 Ultra Lo and Hi SM57", "amps/ampeg_svt/ampeg_svt_gain_10_ultra_lo_and_hi_sm57.nam"),
    ("ampeg_svt_gain_10_ultra_lo_and_h_110852", "Ampeg SVT - Gain 10 Ultra Lo and Hi MD 421", "amps/ampeg_svt/ampeg_svt_gain_10_ultra_lo_and_hi_md_421.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("ampeg_svt_ultra_hi_md_421"),
        &[
            ("ampeg_svt_ultra_hi_md_421", "Ampeg SVT - Ultra Hi MD 421"),
            ("ampeg_svt_ultra_lo_sm57", "Ampeg SVT - Ultra Lo SM57"),
            ("ampeg_svt_sm75", "Ampeg SVT - SM75"),
            ("ampeg_svt_ultra_lo_md_421", "Ampeg SVT - Ultra Lo MD 421"),
            ("ampeg_svt_md_421", "Ampeg SVT - MD 421"),
            ("ampeg_svt_ultra_hi_sm57", "Ampeg SVT - Ultra Hi SM57"),
            ("ampeg_svt_gain_10_ultra_lo_and_h", "Ampeg SVT - Gain 10 Ultra Lo and Hi SM57"),
            ("ampeg_svt_gain_10_ultra_lo_and_h_110852", "Ampeg SVT - Gain 10 Ultra Lo and Hi MD 421"),
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
