use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_pete_cornish_g_2";
pub const DISPLAY_NAME: &str = "Pete Cornish G-2";
const BRAND: &str = "cornish";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "s00_t00_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t00_v10_a1.nam" },
    NamCapture { tone: "s00_t02_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t02_v10_a1.nam" },
    NamCapture { tone: "s00_t04_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t04_v10_a1.nam" },
    NamCapture { tone: "s00_t06_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t06_v10_a1.nam" },
    NamCapture { tone: "s00_t08_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t08_v10_a1.nam" },
    NamCapture { tone: "s00_t10_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t10_v10_a1.nam" },
    NamCapture { tone: "s02_t00_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t00_v10_a1.nam" },
    NamCapture { tone: "s02_t02_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t02_v10_a1.nam" },
    NamCapture { tone: "s02_t04_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t04_v10_a1.nam" },
    NamCapture { tone: "s02_t06_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t06_v10_a1.nam" },
    NamCapture { tone: "s02_t08_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t08_v10_a1.nam" },
    NamCapture { tone: "s02_t10_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t10_v10_a1.nam" },
    NamCapture { tone: "s04_t00_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t00_v10_a1.nam" },
    NamCapture { tone: "s04_t02_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t02_v10_a1.nam" },
    NamCapture { tone: "s04_t04_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t04_v10_a1.nam" },
    NamCapture { tone: "s04_t06_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t06_v10_a1.nam" },
    NamCapture { tone: "s04_t08_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t08_v10_a1.nam" },
    NamCapture { tone: "s04_t10_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t10_v10_a1.nam" },
    NamCapture { tone: "s06_t00_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t00_v10_a1.nam" },
    NamCapture { tone: "s06_t02_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t02_v10_a1.nam" },
    NamCapture { tone: "s06_t04_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t04_v10_a1.nam" },
    NamCapture { tone: "s06_t06_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t06_v10_a1.nam" },
    NamCapture { tone: "s06_t08_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t08_v10_a1.nam" },
    NamCapture { tone: "s06_t10_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t10_v10_a1.nam" },
    NamCapture { tone: "s08_t00_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t00_v10_a1.nam" },
    NamCapture { tone: "s08_t02_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t02_v10_a1.nam" },
    NamCapture { tone: "s08_t04_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t04_v10_a1.nam" },
    NamCapture { tone: "s08_t06_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t06_v10_a1.nam" },
    NamCapture { tone: "s08_t08_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t08_v10_a1.nam" },
    NamCapture { tone: "s08_t10_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t10_v10_a1.nam" },
    NamCapture { tone: "s10_t00_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t00_v10_a1.nam" },
    NamCapture { tone: "s10_t02_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t02_v10_a1.nam" },
    NamCapture { tone: "s10_t04_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t04_v10_a1.nam" },
    NamCapture { tone: "s10_t06_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t06_v10_a1.nam" },
    NamCapture { tone: "s10_t08_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t08_v10_a1.nam" },
    NamCapture { tone: "s10_t10_v10_a1", model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t10_v10_a1.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("s00_t00_v10_a1"),
        &[
            ("s00_t00_v10_a1", "S00 T00 V10 A1"),
            ("s00_t02_v10_a1", "S00 T02 V10 A1"),
            ("s00_t04_v10_a1", "S00 T04 V10 A1"),
            ("s00_t06_v10_a1", "S00 T06 V10 A1"),
            ("s00_t08_v10_a1", "S00 T08 V10 A1"),
            ("s00_t10_v10_a1", "S00 T10 V10 A1"),
            ("s02_t00_v10_a1", "S02 T00 V10 A1"),
            ("s02_t02_v10_a1", "S02 T02 V10 A1"),
            ("s02_t04_v10_a1", "S02 T04 V10 A1"),
            ("s02_t06_v10_a1", "S02 T06 V10 A1"),
            ("s02_t08_v10_a1", "S02 T08 V10 A1"),
            ("s02_t10_v10_a1", "S02 T10 V10 A1"),
            ("s04_t00_v10_a1", "S04 T00 V10 A1"),
            ("s04_t02_v10_a1", "S04 T02 V10 A1"),
            ("s04_t04_v10_a1", "S04 T04 V10 A1"),
            ("s04_t06_v10_a1", "S04 T06 V10 A1"),
            ("s04_t08_v10_a1", "S04 T08 V10 A1"),
            ("s04_t10_v10_a1", "S04 T10 V10 A1"),
            ("s06_t00_v10_a1", "S06 T00 V10 A1"),
            ("s06_t02_v10_a1", "S06 T02 V10 A1"),
            ("s06_t04_v10_a1", "S06 T04 V10 A1"),
            ("s06_t06_v10_a1", "S06 T06 V10 A1"),
            ("s06_t08_v10_a1", "S06 T08 V10 A1"),
            ("s06_t10_v10_a1", "S06 T10 V10 A1"),
            ("s08_t00_v10_a1", "S08 T00 V10 A1"),
            ("s08_t02_v10_a1", "S08 T02 V10 A1"),
            ("s08_t04_v10_a1", "S08 T04 V10 A1"),
            ("s08_t06_v10_a1", "S08 T06 V10 A1"),
            ("s08_t08_v10_a1", "S08 T08 V10 A1"),
            ("s08_t10_v10_a1", "S08 T10 V10 A1"),
            ("s10_t00_v10_a1", "S10 T00 V10 A1"),
            ("s10_t02_v10_a1", "S10 T02 V10 A1"),
            ("s10_t04_v10_a1", "S10 T04 V10 A1"),
            ("s10_t06_v10_a1", "S10 T06 V10 A1"),
            ("s10_t08_v10_a1", "S10 T08 V10 A1"),
            ("s10_t10_v10_a1", "S10 T10 V10 A1"),
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
