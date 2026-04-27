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

pub const MODEL_ID: &str = "nam_pete_cornish_ss_3";
pub const DISPLAY_NAME: &str = "Pete Cornish SS-3";
const BRAND: &str = "pete_cornish";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    bass: f32,
    sustain: f32,
    tone: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Standard,
}

const BASS_MIN: f32 = 0.0;
const BASS_MAX: f32 = 10.0;
const SUSTAIN_MIN: f32 = 0.0;
const SUSTAIN_MAX: f32 = 10.0;
const TONE_MIN: f32 = 0.0;
const TONE_MAX: f32 = 10.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { bass: 0.0, sustain: 0.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s00_b00_t06_v10_a1.nam" },
    GridCapture { bass: 10.0, sustain: 0.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s00_b10_t06_v10_a1.nam" },
    GridCapture { bass: 2.0, sustain: 2.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s02_b02_t04_v10_a1.nam" },
    GridCapture { bass: 2.0, sustain: 2.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s02_b02_t10_v10_a1.nam" },
    GridCapture { bass: 10.0, sustain: 2.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s02_b10_t06_v10_a1.nam" },
    GridCapture { bass: 4.0, sustain: 4.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s04_b04_t06_v10_a1.nam" },
    GridCapture { bass: 10.0, sustain: 4.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s04_b10_t06_v10_a1.nam" },
    GridCapture { bass: 0.0, sustain: 6.0, tone: 0.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b00_t00_v10_a1.nam" },
    GridCapture { bass: 0.0, sustain: 6.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b00_t02_v10_a1.nam" },
    GridCapture { bass: 0.0, sustain: 6.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b00_t04_v10_a1.nam" },
    GridCapture { bass: 0.0, sustain: 6.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b00_t06_v10_a1.nam" },
    GridCapture { bass: 6.0, sustain: 6.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b06_t04_v10_a1.nam" },
    GridCapture { bass: 6.0, sustain: 6.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b06_t10_v10_a1.nam" },
    GridCapture { bass: 10.0, sustain: 6.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s06_b10_t06_v10_a1.nam" },
    GridCapture { bass: 0.0, sustain: 8.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s08_b00_t10_v10_a1.nam" },
    GridCapture { bass: 4.0, sustain: 8.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s08_b04_t06_v10_a1.nam" },
    GridCapture { bass: 10.0, sustain: 8.0, tone: 0.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s08_b10_t00_v10_a1.nam" },
    GridCapture { bass: 0.0, sustain: 10.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s10_b00_t06_v10_a1.nam" },
    GridCapture { bass: 6.0, sustain: 10.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s10_b06_t06_v10_a1.nam" },
    GridCapture { bass: 10.0, sustain: 10.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/pete_cornish_ss_3/tts_corn_ss3_s10_b10_t10_v10_a1.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("bass", "Bass", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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
    let bass_pct = required_f32(params, "bass").map_err(anyhow::Error::msg)?;
    let sustain_pct = required_f32(params, "sustain").map_err(anyhow::Error::msg)?;
    let tone_pct = required_f32(params, "tone").map_err(anyhow::Error::msg)?;
    let bass = BASS_MIN + (bass_pct / 100.0) * (BASS_MAX - BASS_MIN);
    let sustain = SUSTAIN_MIN + (sustain_pct / 100.0) * (SUSTAIN_MAX - SUSTAIN_MIN);
    let tone = TONE_MIN + (tone_pct / 100.0) * (TONE_MAX - TONE_MIN);
    let _size = NamSize::Standard;
    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::Standard);
    candidates
        .min_by(|a, b| {
            let da = (a.bass - bass).powi(2) + (a.sustain - sustain).powi(2) + (a.tone - tone).powi(2);
            let db = (b.bass - bass).powi(2) + (b.sustain - sustain).powi(2) + (b.tone - tone).powi(2);
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

