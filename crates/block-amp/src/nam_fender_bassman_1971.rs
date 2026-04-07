use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};

use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "fender_bassman_1971";
pub const DISPLAY_NAME: &str = "Bassman 1971";
const BRAND: &str = "fender";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct BassmanCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[BassmanCapture] = &[
    BassmanCapture { tone: "clean",           model_path: "full_rigs/fender_bassman_1971/bassman_clean.nam" },
    BassmanCapture { tone: "bright_clean",    model_path: "full_rigs/fender_bassman_1971/bassman_bright_clean.nam" },
    BassmanCapture { tone: "warm_clean",      model_path: "full_rigs/fender_bassman_1971/bassman_warm_clean.nam" },
    BassmanCapture { tone: "sweet_spot",      model_path: "full_rigs/fender_bassman_1971/bassman_sweet_spot.nam" },
    BassmanCapture { tone: "warm_sweet_spot", model_path: "full_rigs/fender_bassman_1971/bassman_warm_sweet_spot.nam" },
    BassmanCapture { tone: "cranked",         model_path: "full_rigs/fender_bassman_1971/bassman_cranked.nam" },
    BassmanCapture { tone: "80s_clean",       model_path: "full_rigs/fender_bassman_1971/bassman_80s_clean.nam" },
    BassmanCapture { tone: "big_clean",       model_path: "full_rigs/fender_bassman_1971/bassman_big_clean.nam" },
    BassmanCapture { tone: "warm_fuzz",       model_path: "full_rigs/fender_bassman_1971/bassman_warm_fuzz.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Rig"),
        Some("sweet_spot"),
        &[
            ("clean",           "Clean"),
            ("bright_clean",    "Bright Clean"),
            ("warm_clean",      "Warm Clean"),
            ("sweet_spot",      "Sweet Spot"),
            ("warm_sweet_spot", "Warm Sweet Spot"),
            ("cranked",         "Cranked"),
            ("80s_clean",       "80s Clean"),
            ("big_clean",       "Big Clean"),
            ("warm_fuzz",       "Warm Fuzz"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static BassmanCapture> {
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.tone == tone)
        .ok_or_else(|| anyhow!("amp model '{}' does not support tone='{}'", MODEL_ID, tone))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
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
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS,
    knob_layout: &[],
};
