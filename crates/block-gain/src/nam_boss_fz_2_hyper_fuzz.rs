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

pub const MODEL_ID: &str = "nam_boss_fz_2_hyper_fuzz";
pub const DISPLAY_NAME: &str = "Boss FZ-2 Hyper Fuzz";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    bass: f32,
    gain: f32,
    tone: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Standard,
}

const BASS_MIN: f32 = 7.0;
const BASS_MAX: f32 = 8.0;
const GAIN_MIN: f32 = 5.0;
const GAIN_MAX: f32 = 10.0;
const TONE_MIN: f32 = 5.0;
const TONE_MAX: f32 = 6.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { bass: 8.0, gain: 10.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/boss_fz_2_hyper_fuzz/fz_2_boost_b8_t6_g10.nam" },
    GridCapture { bass: 7.0, gain: 10.0, tone: 5.0, size: NamSize::Standard, model_path: "pedals/boss_fz_2_hyper_fuzz/fz_2_fuzz_i_l8_b7_t5_g10.nam" },
    GridCapture { bass: 7.0, gain: 5.0, tone: 5.0, size: NamSize::Standard, model_path: "pedals/boss_fz_2_hyper_fuzz/fz_2_fuzz_i_l8_b7_t5_g5.nam" },
    GridCapture { bass: 7.0, gain: 10.0, tone: 5.0, size: NamSize::Standard, model_path: "pedals/boss_fz_2_hyper_fuzz/fz_2_fuzz_ii_l8_b7_t5_g10.nam" },
    GridCapture { bass: 7.0, gain: 5.0, tone: 5.0, size: NamSize::Standard, model_path: "pedals/boss_fz_2_hyper_fuzz/fz_2_fuzz_ii_l8_b7_t5_g5.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("bass", "Bass", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("gain", "Gain", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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
    let gain_pct = required_f32(params, "gain").map_err(anyhow::Error::msg)?;
    let tone_pct = required_f32(params, "tone").map_err(anyhow::Error::msg)?;
    let bass = BASS_MIN + (bass_pct / 100.0) * (BASS_MAX - BASS_MIN);
    let gain = GAIN_MIN + (gain_pct / 100.0) * (GAIN_MAX - GAIN_MIN);
    let tone = TONE_MIN + (tone_pct / 100.0) * (TONE_MAX - TONE_MIN);
    let _size = NamSize::Standard;
    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::Standard);
    candidates
        .min_by(|a, b| {
            let da = (a.bass - bass).powi(2) + (a.gain - gain).powi(2) + (a.tone - tone).powi(2);
            let db = (b.bass - bass).powi(2) + (b.gain - gain).powi(2) + (b.tone - tone).powi(2);
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

