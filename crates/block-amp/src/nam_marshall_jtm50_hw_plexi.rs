use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_jtm50_hw_plexi";
pub const DISPLAY_NAME: &str = "JTM50 HW (Plexi)";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: tone × bright switch. Full 3×2 cartesian (Angus Tone replica).
const CAPTURES: &[(&str, &str, &str)] = &[
    // (tone, bright, file)
    ("v5", "off", "amps/marshall_jtm50_hw_plexi/jtm50_replica_angus_tone_v5.nam"),
    ("v5", "on",  "amps/marshall_jtm50_hw_plexi/jtm50_replica_angus_tone_v5_bright.nam"),
    ("v6", "off", "amps/marshall_jtm50_hw_plexi/jtm50_replica_angus_tone_v6.nam"),
    ("v6", "on",  "amps/marshall_jtm50_hw_plexi/jtm50_replica_angus_tone_v6_bright.nam"),
    ("v7", "off", "amps/marshall_jtm50_hw_plexi/jtm50_replica_angus_tone_v7.nam"),
    ("v7", "on",  "amps/marshall_jtm50_hw_plexi/jtm50_replica_angus_tone_v7_bright.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "tone",
            "Tone",
            Some("Amp"),
            Some("v6"),
            &[
                ("v5", "V5 (Lower)"),
                ("v6", "V6"),
                ("v7", "V7 (Higher)"),
            ],
        ),
        enum_parameter(
            "bright",
            "Bright",
            Some("Amp"),
            Some("off"),
            &[
                ("off", "Off"),
                ("on",  "On"),
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
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    let bright = required_string(params, "bright").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(t, b, _)| *t == tone && *b == bright)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for tone={} bright={}",
                MODEL_ID, tone, bright
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
