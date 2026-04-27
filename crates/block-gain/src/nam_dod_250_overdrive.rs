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

pub const MODEL_ID: &str = "nam_dod_250_overdrive";
pub const DISPLAY_NAME: &str = "DOD 250 Overdrive";
const BRAND: &str = "dod";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    gain: f32,
    level: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Standard,
}

const GAIN_MIN: f32 = 0.0;
const GAIN_MAX: f32 = 10.0;
const LEVEL_MIN: f32 = 8.0;
const LEVEL_MAX: f32 = 10.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { gain: 0.0, level: 10.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_0_l_10_ttsv10.nam" },
    GridCapture { gain: 10.0, level: 10.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_10_l_10_ttsv10.nam" },
    GridCapture { gain: 10.0, level: 8.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_10_l_8_ttsv10.nam" },
    GridCapture { gain: 2.0, level: 10.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_2_l_10_ttsv10.nam" },
    GridCapture { gain: 2.0, level: 8.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_2_l_8_ttsv10.nam" },
    GridCapture { gain: 3.0, level: 10.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_3_l_10_ttsv10.nam" },
    GridCapture { gain: 3.0, level: 8.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_3_l_8_ttsv10.nam" },
    GridCapture { gain: 4.0, level: 10.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_4_l_10_ttsv10.nam" },
    GridCapture { gain: 4.0, level: 8.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_4_l_8_ttsv10.nam" },
    GridCapture { gain: 5.0, level: 10.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_5_l_10_ttsv10.nam" },
    GridCapture { gain: 5.0, level: 8.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_5_l_8_ttsv10.nam" },
    GridCapture { gain: 6.0, level: 10.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_6_l_10_ttsv10.nam" },
    GridCapture { gain: 6.0, level: 8.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_6_l_8_ttsv10.nam" },
    GridCapture { gain: 7.0, level: 10.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_7_l_10_ttsv10.nam" },
    GridCapture { gain: 7.0, level: 8.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_7_l_8_ttsv10.nam" },
    GridCapture { gain: 8.0, level: 10.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_8_l_10_ttsv10.nam" },
    GridCapture { gain: 8.0, level: 8.0, size: NamSize::Standard, model_path: "pedals/dod_250_overdrive/dod_250_g_8_l_8_ttsv10.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("gain", "Gain", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("level", "Level", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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
    let gain_pct = required_f32(params, "gain").map_err(anyhow::Error::msg)?;
    let level_pct = required_f32(params, "level").map_err(anyhow::Error::msg)?;
    let gain = GAIN_MIN + (gain_pct / 100.0) * (GAIN_MAX - GAIN_MIN);
    let level = LEVEL_MIN + (level_pct / 100.0) * (LEVEL_MAX - LEVEL_MIN);
    let _size = NamSize::Standard;
    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::Standard);
    candidates
        .min_by(|a, b| {
            let da = (a.gain - gain).powi(2) + (a.level - level).powi(2);
            let db = (b.gain - gain).powi(2) + (b.level - level).powi(2);
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

