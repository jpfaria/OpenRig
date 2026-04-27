use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_tone_bender";
pub const DISPLAY_NAME: &str = "Tone Bender";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "01_12v", model_path: "pedals/tone_bender/boss_tb_2w_01_12v.nam" },
    NamCapture { tone: "01_7v",  model_path: "pedals/tone_bender/boss_tb_2w_01_7v.nam" },
    NamCapture { tone: "01_9v",  model_path: "pedals/tone_bender/boss_tb_2w_01_9v.nam" },
    NamCapture { tone: "02_12v", model_path: "pedals/tone_bender/boss_tb_2w_02_12v.nam" },
    NamCapture { tone: "02_7v",  model_path: "pedals/tone_bender/boss_tb_2w_02_7v.nam" },
    NamCapture { tone: "02_9v",  model_path: "pedals/tone_bender/boss_tb_2w_02_9v.nam" },
    NamCapture { tone: "03_12v", model_path: "pedals/tone_bender/boss_tb_2w_03_12v.nam" },
    NamCapture { tone: "03_7v",  model_path: "pedals/tone_bender/boss_tb_2w_03_7v.nam" },
    NamCapture { tone: "03_9v",  model_path: "pedals/tone_bender/boss_tb_2w_03_9v.nam" },
    NamCapture { tone: "04_12v", model_path: "pedals/tone_bender/boss_tb_2w_04_12v.nam" },
    NamCapture { tone: "04_7v",  model_path: "pedals/tone_bender/boss_tb_2w_04_7v.nam" },
    NamCapture { tone: "04_9v",  model_path: "pedals/tone_bender/boss_tb_2w_04_9v.nam" },
    NamCapture { tone: "05_12v", model_path: "pedals/tone_bender/boss_tb_2w_05_12v.nam" },
    NamCapture { tone: "05_7v",  model_path: "pedals/tone_bender/boss_tb_2w_05_7v.nam" },
    NamCapture { tone: "05_9v",  model_path: "pedals/tone_bender/boss_tb_2w_05_9v.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("01_12v"),
        &[
            ("01_12v", "01 12V"),
            ("01_7v",  "01 7V"),
            ("01_9v",  "01 9V"),
            ("02_12v", "02 12V"),
            ("02_7v",  "02 7V"),
            ("02_9v",  "02 9V"),
            ("03_12v", "03 12V"),
            ("03_7v",  "03 7V"),
            ("03_9v",  "03 9V"),
            ("04_12v", "04 12V"),
            ("04_7v",  "04 7V"),
            ("04_9v",  "04 9V"),
            ("05_12v", "05 12V"),
            ("05_7v",  "05 7V"),
            ("05_9v",  "05 9V"),
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
