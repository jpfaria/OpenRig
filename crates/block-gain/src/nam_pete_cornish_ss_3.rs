use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_pete_cornish_ss_3";
pub const DISPLAY_NAME: &str = "Pete Cornish SS-3";
const BRAND: &str = "cornish";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "s00_b00_t06_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s00_b00_t06_v10_a1.nam" },
    NamCapture { tone: "s00_b10_t06_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s00_b10_t06_v10_a1.nam" },
    NamCapture { tone: "s02_b02_t04_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s02_b02_t04_v10_a1.nam" },
    NamCapture { tone: "s02_b02_t10_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s02_b02_t10_v10_a1.nam" },
    NamCapture { tone: "s02_b10_t06_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s02_b10_t06_v10_a1.nam" },
    NamCapture { tone: "s04_b04_t06_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s04_b04_t06_v10_a1.nam" },
    NamCapture { tone: "s04_b10_t06_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s04_b10_t06_v10_a1.nam" },
    NamCapture { tone: "s06_b00_t00_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b00_t00_v10_a1.nam" },
    NamCapture { tone: "s06_b00_t02_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b00_t02_v10_a1.nam" },
    NamCapture { tone: "s06_b00_t04_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b00_t04_v10_a1.nam" },
    NamCapture { tone: "s06_b00_t06_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b00_t06_v10_a1.nam" },
    NamCapture { tone: "s06_b06_t04_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b06_t04_v10_a1.nam" },
    NamCapture { tone: "s06_b06_t10_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b06_t10_v10_a1.nam" },
    NamCapture { tone: "s06_b10_t06_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b10_t06_v10_a1.nam" },
    NamCapture { tone: "s08_b00_t10_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s08_b00_t10_v10_a1.nam" },
    NamCapture { tone: "s08_b04_t06_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s08_b04_t06_v10_a1.nam" },
    NamCapture { tone: "s08_b10_t00_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s08_b10_t00_v10_a1.nam" },
    NamCapture { tone: "s10_b00_t06_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s10_b00_t06_v10_a1.nam" },
    NamCapture { tone: "s10_b06_t06_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s10_b06_t06_v10_a1.nam" },
    NamCapture { tone: "s10_b10_t10_v10_a1", model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s10_b10_t10_v10_a1.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("s00_b00_t06_v10_a1"),
        &[
            ("s00_b00_t06_v10_a1", "S00 B00 T06 V10 A1"),
            ("s00_b10_t06_v10_a1", "S00 B10 T06 V10 A1"),
            ("s02_b02_t04_v10_a1", "S02 B02 T04 V10 A1"),
            ("s02_b02_t10_v10_a1", "S02 B02 T10 V10 A1"),
            ("s02_b10_t06_v10_a1", "S02 B10 T06 V10 A1"),
            ("s04_b04_t06_v10_a1", "S04 B04 T06 V10 A1"),
            ("s04_b10_t06_v10_a1", "S04 B10 T06 V10 A1"),
            ("s06_b00_t00_v10_a1", "S06 B00 T00 V10 A1"),
            ("s06_b00_t02_v10_a1", "S06 B00 T02 V10 A1"),
            ("s06_b00_t04_v10_a1", "S06 B00 T04 V10 A1"),
            ("s06_b00_t06_v10_a1", "S06 B00 T06 V10 A1"),
            ("s06_b06_t04_v10_a1", "S06 B06 T04 V10 A1"),
            ("s06_b06_t10_v10_a1", "S06 B06 T10 V10 A1"),
            ("s06_b10_t06_v10_a1", "S06 B10 T06 V10 A1"),
            ("s08_b00_t10_v10_a1", "S08 B00 T10 V10 A1"),
            ("s08_b04_t06_v10_a1", "S08 B04 T06 V10 A1"),
            ("s08_b10_t00_v10_a1", "S08 B10 T00 V10 A1"),
            ("s10_b00_t06_v10_a1", "S10 B00 T06 V10 A1"),
            ("s10_b06_t06_v10_a1", "S10 B06 T06 V10 A1"),
            ("s10_b10_t10_v10_a1", "S10 B10 T10 V10 A1"),
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
