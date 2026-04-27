use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{
    float_parameter, required_f32, 
    ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "nam_pete_cornish_g_2";
pub const DISPLAY_NAME: &str = "Pete Cornish G-2";
const BRAND: &str = "pete_cornish";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    sustain: f32,
    tone: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Standard,
}

const SUSTAIN_MIN: f32 = 0.0;
const SUSTAIN_MAX: f32 = 10.0;
const TONE_MIN: f32 = 0.0;
const TONE_MAX: f32 = 10.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { sustain: 0.0, tone: 0.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t00_v10_a1.nam" },
    GridCapture { sustain: 0.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t02_v10_a1.nam" },
    GridCapture { sustain: 0.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t04_v10_a1.nam" },
    GridCapture { sustain: 0.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t06_v10_a1.nam" },
    GridCapture { sustain: 0.0, tone: 8.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t08_v10_a1.nam" },
    GridCapture { sustain: 0.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s00_t10_v10_a1.nam" },
    GridCapture { sustain: 2.0, tone: 0.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t00_v10_a1.nam" },
    GridCapture { sustain: 2.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t02_v10_a1.nam" },
    GridCapture { sustain: 2.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t04_v10_a1.nam" },
    GridCapture { sustain: 2.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t06_v10_a1.nam" },
    GridCapture { sustain: 2.0, tone: 8.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t08_v10_a1.nam" },
    GridCapture { sustain: 2.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s02_t10_v10_a1.nam" },
    GridCapture { sustain: 4.0, tone: 0.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t00_v10_a1.nam" },
    GridCapture { sustain: 4.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t02_v10_a1.nam" },
    GridCapture { sustain: 4.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t04_v10_a1.nam" },
    GridCapture { sustain: 4.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t06_v10_a1.nam" },
    GridCapture { sustain: 4.0, tone: 8.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t08_v10_a1.nam" },
    GridCapture { sustain: 4.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s04_t10_v10_a1.nam" },
    GridCapture { sustain: 6.0, tone: 0.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t00_v10_a1.nam" },
    GridCapture { sustain: 6.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t02_v10_a1.nam" },
    GridCapture { sustain: 6.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t04_v10_a1.nam" },
    GridCapture { sustain: 6.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t06_v10_a1.nam" },
    GridCapture { sustain: 6.0, tone: 8.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t08_v10_a1.nam" },
    GridCapture { sustain: 6.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s06_t10_v10_a1.nam" },
    GridCapture { sustain: 8.0, tone: 0.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t00_v10_a1.nam" },
    GridCapture { sustain: 8.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t02_v10_a1.nam" },
    GridCapture { sustain: 8.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t04_v10_a1.nam" },
    GridCapture { sustain: 8.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t06_v10_a1.nam" },
    GridCapture { sustain: 8.0, tone: 8.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t08_v10_a1.nam" },
    GridCapture { sustain: 8.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s08_t10_v10_a1.nam" },
    GridCapture { sustain: 10.0, tone: 0.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t00_v10_a1.nam" },
    GridCapture { sustain: 10.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t02_v10_a1.nam" },
    GridCapture { sustain: 10.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t04_v10_a1.nam" },
    GridCapture { sustain: 10.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t06_v10_a1.nam" },
    GridCapture { sustain: 10.0, tone: 8.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t08_v10_a1.nam" },
    GridCapture { sustain: 10.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_g_2/tts_cornish_g2_s10_t10_v10_a1.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("sustain", "Sustain", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("tone", "Tone", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
    ];
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static GridCapture> {
    let sustain_pct = required_f32(params, "sustain").map_err(anyhow::Error::msg)?;
    let tone_pct = required_f32(params, "tone").map_err(anyhow::Error::msg)?;
    let sustain = SUSTAIN_MIN + (sustain_pct / 100.0) * (SUSTAIN_MAX - SUSTAIN_MIN);
    let tone = TONE_MIN + (tone_pct / 100.0) * (TONE_MAX - TONE_MIN);
    let _size = NamSize::Standard;
    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::Standard);
    candidates
        .min_by(|a, b| {
            let da = (a.sustain - sustain).powi(2) + (a.tone - tone).powi(2);
            let db = (b.sustain - sustain).powi(2) + (b.tone - tone).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| anyhow!("no capture matches"))
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

