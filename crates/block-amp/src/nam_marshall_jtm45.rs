use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_jtm45";
pub const DISPLAY_NAME: &str = "JTM45";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: channel × volume.
// Channels: Normal, Treble, Blend (jumped Normal+Treble).
// Volume value (5 / 10) corresponds to the original front-panel knob.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (channel, volume, file)
    ("normal", "5",  "amps/marshall_jtm45/marshall_jtm45_jtm45_normal5_jtm45_n5.nam"),
    ("normal", "10", "amps/marshall_jtm45/marshall_jtm45_jtm45_normal10_jtm45_n10.nam"),
    ("treble", "5",  "amps/marshall_jtm45/marshall_jtm45_jtm45_treble5_jtm45_t5.nam"),
    ("treble", "10", "amps/marshall_jtm45/marshall_jtm45_jtm45_treble10_jtm45_t10.nam"),
    ("blend",  "5",  "amps/marshall_jtm45/marshall_jtm45_jtm45_blend_treb5_norm5_jtm45_t5n5.nam"),
    ("blend",  "10", "amps/marshall_jtm45/marshall_jtm45_jtm45_blend_treb10_norm10_jtm45_t10n10.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("blend"),
            &[
                ("normal", "Normal"),
                ("treble", "Treble"),
                ("blend",  "Blend"),
            ],
        ),
        enum_parameter(
            "volume",
            "Volume",
            Some("Amp"),
            Some("5"),
            &[
                ("5",  "5"),
                ("10", "10"),
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
    let volume = required_string(params, "volume").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(c, v, _)| *c == channel && *v == volume)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for channel={} volume={}",
                MODEL_ID, channel, volume
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
