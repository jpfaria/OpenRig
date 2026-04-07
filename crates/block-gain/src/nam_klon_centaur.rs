use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "klon_centaur";
pub const DISPLAY_NAME: &str = "Klon Centaur Silver";
const BRAND: &str = "klon";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct KlonCapture {
    setting: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[KlonCapture] = &[
    KlonCapture { setting: "255",        model_path: "pedals/klon_centaur/klon_255.nam" },
    KlonCapture { setting: "277",        model_path: "pedals/klon_centaur/klon_277.nam" },
    KlonCapture { setting: "468",        model_path: "pedals/klon_centaur/klon_468.nam" },
    KlonCapture { setting: "555",        model_path: "pedals/klon_centaur/klon_555.nam" },
    KlonCapture { setting: "668",        model_path: "pedals/klon_centaur/klon_668.nam" },
    KlonCapture { setting: "john_mayer", model_path: "pedals/klon_centaur/klon_john_mayer.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "setting",
        "Setting",
        Some("Pedal"),
        Some("555"),
        &[
            ("255",        "T2 G5 O5"),
            ("277",        "T2 G7 O7"),
            ("468",        "T4 G6 O8"),
            ("555",        "T5 G5 O5"),
            ("668",        "T6 G6 O8"),
            ("john_mayer", "John Mayer"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static KlonCapture> {
    let setting = required_string(params, "setting").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.setting == setting)
        .ok_or_else(|| anyhow!("gain model '{}' does not support setting='{}'", MODEL_ID, setting))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
