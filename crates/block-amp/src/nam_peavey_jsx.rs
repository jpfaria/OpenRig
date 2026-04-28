use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_peavey_jsx";
pub const DISPLAY_NAME: &str = "JSX";
const BRAND: &str = "peavey";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: channel × eq. Full 2×2 grid.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (channel, eq, file)
    ("crunch", "mid",  "amps/peavey_jsx/peavey_jsx_crunch_mid.nam"),
    ("crunch", "high", "amps/peavey_jsx/peavey_jsx_crunch_high.nam"),
    ("ultra",  "mid",  "amps/peavey_jsx/peavey_jsx_ultra_mid.nam"),
    ("ultra",  "high", "amps/peavey_jsx/peavey_jsx_ultra_high.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("crunch"),
            &[
                ("crunch", "Crunch"),
                ("ultra",  "Ultra"),
            ],
        ),
        enum_parameter(
            "eq",
            "EQ",
            Some("Amp"),
            Some("mid"),
            &[
                ("mid",  "Mid"),
                ("high", "High"),
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
    let eq = required_string(params, "eq").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(c, e, _)| *c == channel && *e == eq)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for channel={} eq={}",
                MODEL_ID, channel, eq
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
