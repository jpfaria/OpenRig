use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_digitech_grunge";
pub const DISPLAY_NAME: &str = "DigiTech Grunge";
const BRAND: &str = "digitech";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "5_grunge_3", model_path: "pedals/digitech_grunge/digitech_grunge_tone_5_grunge_3.nam" },
    NamCapture { tone: "5_grunge_5", model_path: "pedals/digitech_grunge/digitech_grunge_tone_5_grunge_5.nam" },
    NamCapture { tone: "5_grunge_7", model_path: "pedals/digitech_grunge/digitech_grunge_tone_5_grunge_7.nam" },
    NamCapture { tone: "5_grunge_9", model_path: "pedals/digitech_grunge/digitech_grunge_tone_5_grunge_9.nam" },
    NamCapture { tone: "7_grunge_3", model_path: "pedals/digitech_grunge/digitech_grunge_tone_7_grunge_3.nam" },
    NamCapture { tone: "7_grunge_5", model_path: "pedals/digitech_grunge/digitech_grunge_tone_7_grunge_5.nam" },
    NamCapture { tone: "7_grunge_7", model_path: "pedals/digitech_grunge/digitech_grunge_tone_7_grunge_7.nam" },
    NamCapture { tone: "7_grunge_9", model_path: "pedals/digitech_grunge/digitech_grunge_tone_7_grunge_9.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("5_grunge_3"),
        &[
            ("5_grunge_3", "5 Grunge 3"),
            ("5_grunge_5", "5 Grunge 5"),
            ("5_grunge_7", "5 Grunge 7"),
            ("5_grunge_9", "5 Grunge 9"),
            ("7_grunge_3", "7 Grunge 3"),
            ("7_grunge_5", "7 Grunge 5"),
            ("7_grunge_7", "7 Grunge 7"),
            ("7_grunge_9", "7 Grunge 9"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static NamCapture> {
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.tone == tone)
        .ok_or_else(|| anyhow!("gain model '{}' does not support tone='{}'", MODEL_ID, tone))
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
