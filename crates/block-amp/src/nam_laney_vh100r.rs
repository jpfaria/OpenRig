use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_laney_vh100r";
pub const DISPLAY_NAME: &str = "VH100R";
const BRAND: &str = "laney";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("neutral_g7", "VH100R - Channel 2 Hi - Neutral G7", "amps/laney_vh100r/vh100r_channel_2_hi_neutral_g7_2.nam"),
    ("neutral_g8", "VH100R - Channel 2 Hi - Neutral G8", "amps/laney_vh100r/vh100r_channel_2_hi_neutral_g8_2.nam"),
    ("treble_g8", "VH100R - Channel 2 Hi - Treble G8", "amps/laney_vh100r/vh100r_channel_2_hi_treble_g8_2.nam"),
    ("treble_g10", "VH100R - Channel 2 Hi - Treble G10", "amps/laney_vh100r/vh100r_channel_2_hi_treble_g10_2.nam"),
    ("neutral_drive_g10", "VH100R - Channel 2 Hi - Neutral Drive G10", "amps/laney_vh100r/vh100r_channel_2_hi_neutral_drive_g10_2.nam"),
    ("neutral_g5", "VH100R - Channel 2 Hi - Neutral G5", "amps/laney_vh100r/vh100r_channel_2_hi_neutral_g5_2.nam"),
    ("neutral_drive_g8", "VH100R - Channel 2 Hi - Neutral Drive G8", "amps/laney_vh100r/vh100r_channel_2_hi_neutral_drive_g8_2.nam"),
    ("neutral_drive_g7", "VH100R - Channel 2 Hi - Neutral Drive G7", "amps/laney_vh100r/vh100r_channel_2_hi_neutral_drive_g7_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("neutral_g7"),
        &[
            ("neutral_g7", "VH100R - Channel 2 Hi - Neutral G7"),
            ("neutral_g8", "VH100R - Channel 2 Hi - Neutral G8"),
            ("treble_g8", "VH100R - Channel 2 Hi - Treble G8"),
            ("treble_g10", "VH100R - Channel 2 Hi - Treble G10"),
            ("neutral_drive_g10", "VH100R - Channel 2 Hi - Neutral Drive G10"),
            ("neutral_g5", "VH100R - Channel 2 Hi - Neutral G5"),
            ("neutral_drive_g8", "VH100R - Channel 2 Hi - Neutral Drive G8"),
            ("neutral_drive_g7", "VH100R - Channel 2 Hi - Neutral Drive G7"),
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
