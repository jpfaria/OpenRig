use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_dumble_steel_string_singer";
pub const DISPLAY_NAME: &str = "Steel String Singer";
const BRAND: &str = "dumble";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: channel × variant. Sparse — clean has Default+Full, drive
// has 1/2/Full. resolve_capture rejects missing combos.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (channel, variant, file)
    ("clean", "default", "amps/dumble_steel_string_singer/dumble_steel_ss_clean.nam"),
    ("clean", "full",    "amps/dumble_steel_string_singer/dumble_steel_ss_clean_full.nam"),
    ("drive", "1",       "amps/dumble_steel_string_singer/dumble_steel_ss_drive_1.nam"),
    ("drive", "2",       "amps/dumble_steel_string_singer/dumble_steel_ss_drive_2.nam"),
    ("drive", "full",    "amps/dumble_steel_string_singer/dumble_steel_ss_drive_full.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("clean"),
            &[
                ("clean", "Clean"),
                ("drive", "Drive"),
            ],
        ),
        enum_parameter(
            "variant",
            "Variant",
            Some("Amp"),
            Some("default"),
            &[
                ("default", "Default"),
                ("1",       "1"),
                ("2",       "2"),
                ("full",    "Full"),
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
    let variant = required_string(params, "variant").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(c, v, _)| *c == channel && *v == variant)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for channel={} variant={}",
                MODEL_ID, channel, variant
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
