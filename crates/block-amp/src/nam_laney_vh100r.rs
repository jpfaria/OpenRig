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

// All captures are Channel 2 Hi. Two-axis pack: voicing × gain step.
// Only 8 of the 3×4 = 12 possible combinations were captured. The
// `resolve_capture` lookup rejects the holes so the UI exposes both
// knobs as independent controls.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, gain, file)
    ("neutral",       "g5",  "amps/laney_vh100r/vh100r_channel_2_hi_neutral_g5.nam"),
    ("neutral",       "g7",  "amps/laney_vh100r/vh100r_channel_2_hi_neutral_g7.nam"),
    ("neutral",       "g8",  "amps/laney_vh100r/vh100r_channel_2_hi_neutral_g8.nam"),
    ("treble",        "g8",  "amps/laney_vh100r/vh100r_channel_2_hi_treble_g8.nam"),
    ("treble",        "g10", "amps/laney_vh100r/vh100r_channel_2_hi_treble_g10.nam"),
    ("neutral_drive", "g7",  "amps/laney_vh100r/vh100r_channel_2_hi_neutral_drive_g7.nam"),
    ("neutral_drive", "g8",  "amps/laney_vh100r/vh100r_channel_2_hi_neutral_drive_g8.nam"),
    ("neutral_drive", "g10", "amps/laney_vh100r/vh100r_channel_2_hi_neutral_drive_g10.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("neutral"),
            &[
                ("neutral",       "Neutral"),
                ("treble",        "Treble"),
                ("neutral_drive", "Neutral Drive"),
            ],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some("g7"),
            &[
                ("g5",  "G5"),
                ("g7",  "G7"),
                ("g8",  "G8"),
                ("g10", "G10"),
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
    let voicing = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, g, _)| *v == voicing && *g == gain)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} gain={}",
                MODEL_ID, voicing, gain
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
