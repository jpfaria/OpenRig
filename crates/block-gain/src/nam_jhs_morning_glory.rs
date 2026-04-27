use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_jhs_morning_glory";
pub const DISPLAY_NAME: &str = "JHS Morning Glory";
const BRAND: &str = "jhs";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "d2_t8",               model_path: "pedals/jhs_morning_glory/jhs_morning_glory_d2_t8.nam" },
    NamCapture { tone: "d2_t8_high_cut",      model_path: "pedals/jhs_morning_glory/jhs_morning_glory_d2_t8_high_cut.nam" },
    NamCapture { tone: "d4_t8",               model_path: "pedals/jhs_morning_glory/jhs_morning_glory_d4_t8.nam" },
    NamCapture { tone: "d4_t8_high_cut",      model_path: "pedals/jhs_morning_glory/jhs_morning_glory_d4_t8_high_cut.nam" },
    NamCapture { tone: "d8_t8",               model_path: "pedals/jhs_morning_glory/jhs_morning_glory_d8_t8.nam" },
    NamCapture { tone: "d8_t8_high_cut",      model_path: "pedals/jhs_morning_glory/jhs_morning_glory_d8_t8_high_cut.nam" },
    NamCapture { tone: "no_tone_d2",          model_path: "pedals/jhs_morning_glory/jhs_morning_glory_no_tone_d2.nam" },
    NamCapture { tone: "no_tone_d2_high_cut", model_path: "pedals/jhs_morning_glory/jhs_morning_glory_no_tone_d2_high_cut.nam" },
    NamCapture { tone: "no_tone_d4",          model_path: "pedals/jhs_morning_glory/jhs_morning_glory_no_tone_d4.nam" },
    NamCapture { tone: "no_tone_d4_high_cut", model_path: "pedals/jhs_morning_glory/jhs_morning_glory_no_tone_d4_high_cut.nam" },
    NamCapture { tone: "no_tone_d8",          model_path: "pedals/jhs_morning_glory/jhs_morning_glory_no_tone_d8.nam" },
    NamCapture { tone: "no_tone_d8_high_cut", model_path: "pedals/jhs_morning_glory/jhs_morning_glory_no_tone_d8_high_cut.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("d2_t8"),
        &[
            ("d2_t8",               "D2 T8"),
            ("d2_t8_high_cut",      "D2 T8 High Cut"),
            ("d4_t8",               "D4 T8"),
            ("d4_t8_high_cut",      "D4 T8 High Cut"),
            ("d8_t8",               "D8 T8"),
            ("d8_t8_high_cut",      "D8 T8 High Cut"),
            ("no_tone_d2",          "No Tone D2"),
            ("no_tone_d2_high_cut", "No Tone D2 High Cut"),
            ("no_tone_d4",          "No Tone D4"),
            ("no_tone_d4_high_cut", "No Tone D4 High Cut"),
            ("no_tone_d8",          "No Tone D8"),
            ("no_tone_d8_high_cut", "No Tone D8 High Cut"),
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
