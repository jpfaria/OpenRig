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

pub const MODEL_ID: &str = "nam_ibanez_tube_screamer";
pub const DISPLAY_NAME: &str = "Ibanez Tube Screamer";
const BRAND: &str = "fortin";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    drive: f32,
    level: f32,
    tone: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Standard,
}

const DRIVE_MIN: f32 = 0.0;
const DRIVE_MAX: f32 = 10.0;
const LEVEL_MIN: f32 = 5.0;
const LEVEL_MAX: f32 = 10.0;
const TONE_MIN: f32 = 5.0;
const TONE_MAX: f32 = 10.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { drive: 0.0, level: 10.0, tone: 5.0, size: NamSize::Standard, model_path: "pedals/ibanez_tube_screamer/d_00_t_05_l_10.nam" },
    GridCapture { drive: 0.0, level: 8.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/ibanez_tube_screamer/d_00_t_06_l_08.nam" },
    GridCapture { drive: 0.0, level: 10.0, tone: 8.0, size: NamSize::Standard, model_path: "pedals/ibanez_tube_screamer/d_00_t_08_l_10.nam" },
    GridCapture { drive: 0.0, level: 10.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/ibanez_tube_screamer/d_00_t_10_l_10.nam" },
    GridCapture { drive: 2.0, level: 8.0, tone: 8.0, size: NamSize::Standard, model_path: "pedals/ibanez_tube_screamer/d_02_t_08_l_08.nam" },
    GridCapture { drive: 2.0, level: 8.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/ibanez_tube_screamer/d_02_t_10_l_08.nam" },
    GridCapture { drive: 5.0, level: 5.0, tone: 8.0, size: NamSize::Standard, model_path: "pedals/ibanez_tube_screamer/d_05_t_08_l_05.nam" },
    GridCapture { drive: 5.0, level: 5.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/ibanez_tube_screamer/d_05_t_10_l_05.nam" },
    GridCapture { drive: 10.0, level: 10.0, tone: 10.0, size: NamSize::Standard, model_path: "pedals/ibanez_tube_screamer/d_10_t_10_l_10.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("drive", "Drive", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("level", "Level", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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
    let drive_pct = required_f32(params, "drive").map_err(anyhow::Error::msg)?;
    let level_pct = required_f32(params, "level").map_err(anyhow::Error::msg)?;
    let tone_pct = required_f32(params, "tone").map_err(anyhow::Error::msg)?;
    let drive = DRIVE_MIN + (drive_pct / 100.0) * (DRIVE_MAX - DRIVE_MIN);
    let level = LEVEL_MIN + (level_pct / 100.0) * (LEVEL_MAX - LEVEL_MIN);
    let tone = TONE_MIN + (tone_pct / 100.0) * (TONE_MAX - TONE_MIN);
    let _size = NamSize::Standard;
    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::Standard);
    candidates
        .min_by(|a, b| {
            let da = (a.drive - drive).powi(2) + (a.level - level).powi(2) + (a.tone - tone).powi(2);
            let db = (b.drive - drive).powi(2) + (b.level - level).powi(2) + (b.tone - tone).powi(2);
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

