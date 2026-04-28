use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_jcm2000_dsl";
pub const DISPLAY_NAME: &str = "JCM2000 DSL";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: channel × style. BMR captures.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (channel, style, file)
    ("clean",  "default", "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_clean_esr0_0020.nam"),
    ("crunch", "default", "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_crunch_esr0_0021.nam"),
    ("od1",    "altrock", "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_od1_altrock_esr0_0055.nam"),
    ("od1",    "dimed",   "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_od1_dimed_esr0_0357.nam"),
    ("od2",    "altrock", "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_od2_altrock_esr0_0050.nam"),
    ("od2",    "dimed",   "amps/marshall_jcm2000_dsl/bmr_marshall_jcm2000_od2_dimed_esr0_0813.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("od2"),
            &[
                ("clean",  "Clean"),
                ("crunch", "Crunch"),
                ("od1",    "OD1"),
                ("od2",    "OD2"),
            ],
        ),
        enum_parameter(
            "style",
            "Style",
            Some("Amp"),
            Some("altrock"),
            &[
                ("default", "Default"),
                ("altrock", "AltRock"),
                ("dimed",   "Dimed"),
            ],
        ),
    ];
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
    let channel = required_string(params, "channel").map_err(anyhow::Error::msg)?;
    let style = required_string(params, "style").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(c, s, _)| *c == channel && *s == style)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for channel={} style={}",
                MODEL_ID, channel, style
            )
        })
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
