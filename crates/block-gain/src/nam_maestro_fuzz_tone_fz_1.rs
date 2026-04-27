use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_maestro_fuzz_tone_fz_1";
pub const DISPLAY_NAME: &str = "Maestro Fuzz-Tone FZ-1";
const BRAND: &str = "maestro";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "modern_high_gain",  model_path: "pedals/maestro_fuzz_tone_fz_1/maestro_fz_m_modern_high_gain.nam" },
    NamCapture { tone: "modern_mid_gain",   model_path: "pedals/maestro_fuzz_tone_fz_1/maestro_fz_m_modern_mid_gain.nam" },
    NamCapture { tone: "vintage_high_gain", model_path: "pedals/maestro_fuzz_tone_fz_1/maestro_fz_m_vintage_high_gain.nam" },
    NamCapture { tone: "vintage_low_gain",  model_path: "pedals/maestro_fuzz_tone_fz_1/maestro_fz_m_vintage_low_gain.nam" },
    NamCapture { tone: "vintage_mid_gain",  model_path: "pedals/maestro_fuzz_tone_fz_1/maestro_fz_m_vintage_mid_gain.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("modern_high_gain"),
        &[
            ("modern_high_gain",  "Modern High Gain"),
            ("modern_mid_gain",   "Modern Mid Gain"),
            ("vintage_high_gain", "Vintage High Gain"),
            ("vintage_low_gain",  "Vintage Low Gain"),
            ("vintage_mid_gain",  "Vintage Mid Gain"),
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
