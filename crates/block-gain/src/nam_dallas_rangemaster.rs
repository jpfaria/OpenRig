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

pub const MODEL_ID: &str = "nam_dallas_rangemaster";
pub const DISPLAY_NAME: &str = "Dallas Rangemaster";
const BRAND: &str = "dallas";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    tone: f32,
    volume: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Standard,
}

const TONE_MIN: f32 = 0.0;
const TONE_MAX: f32 = 10.0;
const VOLUME_MIN: f32 = 3.0;
const VOLUME_MAX: f32 = 10.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { tone: 0.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t0_c.nam" },
    GridCapture { tone: 0.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t0_s.nam" },
    GridCapture { tone: 0.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t0_xs.nam" },
    GridCapture { tone: 10.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t10_main_c.nam" },
    GridCapture { tone: 10.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t10_main_s.nam" },
    GridCapture { tone: 10.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t10_main_xs.nam" },
    GridCapture { tone: 3.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t3_c.nam" },
    GridCapture { tone: 3.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t3_s.nam" },
    GridCapture { tone: 3.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t3_xs.nam" },
    GridCapture { tone: 5.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t5_c.nam" },
    GridCapture { tone: 5.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t5_s.nam" },
    GridCapture { tone: 5.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t5_xs.nam" },
    GridCapture { tone: 7.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t7_c.nam" },
    GridCapture { tone: 7.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t7_s.nam" },
    GridCapture { tone: 7.0, volume: 10.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t7_xs.nam" },
    GridCapture { tone: 10.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t10_main_c.nam" },
    GridCapture { tone: 10.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t10_main_s.nam" },
    GridCapture { tone: 10.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t10_main_xs.nam" },
    GridCapture { tone: 3.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t3_c.nam" },
    GridCapture { tone: 3.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t3_s.nam" },
    GridCapture { tone: 3.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t3_xs.nam" },
    GridCapture { tone: 5.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t5_c.nam" },
    GridCapture { tone: 5.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t5_s.nam" },
    GridCapture { tone: 5.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t5_xs.nam" },
    GridCapture { tone: 7.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t7_c.nam" },
    GridCapture { tone: 7.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t7_s.nam" },
    GridCapture { tone: 7.0, volume: 3.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t7_xs.nam" },
    GridCapture { tone: 10.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t10_main_c.nam" },
    GridCapture { tone: 10.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t10_main_s.nam" },
    GridCapture { tone: 10.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t10_main_xs.nam" },
    GridCapture { tone: 3.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t3_c.nam" },
    GridCapture { tone: 3.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t3_s.nam" },
    GridCapture { tone: 3.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t3_xs.nam" },
    GridCapture { tone: 5.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t5_c.nam" },
    GridCapture { tone: 5.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t5_s.nam" },
    GridCapture { tone: 5.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t5_xs.nam" },
    GridCapture { tone: 7.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t7_c.nam" },
    GridCapture { tone: 7.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t7_s.nam" },
    GridCapture { tone: 7.0, volume: 5.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t7_xs.nam" },
    GridCapture { tone: 0.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t0_c.nam" },
    GridCapture { tone: 0.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t0_s.nam" },
    GridCapture { tone: 0.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t0_xs.nam" },
    GridCapture { tone: 10.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t10_main_c.nam" },
    GridCapture { tone: 10.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t10_main_s.nam" },
    GridCapture { tone: 10.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t10_main_xs.nam" },
    GridCapture { tone: 3.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t3_c.nam" },
    GridCapture { tone: 3.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t3_s.nam" },
    GridCapture { tone: 3.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t3_xs.nam" },
    GridCapture { tone: 5.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t5_c.nam" },
    GridCapture { tone: 5.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t5_s.nam" },
    GridCapture { tone: 5.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t5_xs.nam" },
    GridCapture { tone: 7.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t7_c.nam" },
    GridCapture { tone: 7.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t7_s.nam" },
    GridCapture { tone: 7.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t7_xs.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("tone", "Tone", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("volume", "Volume", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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
    let tone_pct = required_f32(params, "tone").map_err(anyhow::Error::msg)?;
    let volume_pct = required_f32(params, "volume").map_err(anyhow::Error::msg)?;
    let tone = TONE_MIN + (tone_pct / 100.0) * (TONE_MAX - TONE_MIN);
    let volume = VOLUME_MIN + (volume_pct / 100.0) * (VOLUME_MAX - VOLUME_MIN);
    let _size = NamSize::Standard;
    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::Standard);
    candidates
        .min_by(|a, b| {
            let da = (a.tone - tone).powi(2) + (a.volume - volume).powi(2);
            let db = (b.tone - tone).powi(2) + (b.volume - volume).powi(2);
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

