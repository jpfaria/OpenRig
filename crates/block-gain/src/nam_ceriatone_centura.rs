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

pub const MODEL_ID: &str = "nam_ceriatone_centura";
pub const DISPLAY_NAME: &str = "Ceriatone Centura";
const BRAND: &str = "ceriatone";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    gain: f32,
    treble: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Standard,
}

const GAIN_MIN: f32 = 3.0;
const GAIN_MAX: f32 = 12.0;
const TREBLE_MIN: f32 = 1.3;
const TREBLE_MAX: f32 = 12.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { gain: 3.0, treble: 1.3, size: NamSize::Standard, model_path: "pedals/ceriatone_centura/ceriatone_centura_a_a_g_03_00_t_01_30_o_10_30.nam" },
    GridCapture { gain: 3.0, treble: 12.0, size: NamSize::Standard, model_path: "pedals/ceriatone_centura/ceriatone_centura_a_a_g_03_00_t_12_00_o_10_30.nam" },
    GridCapture { gain: 9.0, treble: 1.3, size: NamSize::Standard, model_path: "pedals/ceriatone_centura/ceriatone_centura_a_a_g_09_00_t_01_30_o_10_30.nam" },
    GridCapture { gain: 9.0, treble: 12.0, size: NamSize::Standard, model_path: "pedals/ceriatone_centura/ceriatone_centura_a_a_g_09_00_t_12_00_o_10_30.nam" },
    GridCapture { gain: 12.0, treble: 1.3, size: NamSize::Standard, model_path: "pedals/ceriatone_centura/ceriatone_centura_a_a_g_12_00_t_01_30_o_10_30.nam" },
    GridCapture { gain: 12.0, treble: 12.0, size: NamSize::Standard, model_path: "pedals/ceriatone_centura/ceriatone_centura_a_a_g_12_00_t_12_00_o_10_30.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("gain", "Gain", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("treble", "Treble", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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
    let treble_pct = required_f32(params, "treble").map_err(anyhow::Error::msg)?;
    let gain = GAIN_MIN + (gain_pct / 100.0) * (GAIN_MAX - GAIN_MIN);
    let treble = TREBLE_MIN + (treble_pct / 100.0) * (TREBLE_MAX - TREBLE_MIN);
    let _size = NamSize::Standard;
    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::Standard);
    candidates
        .min_by(|a, b| {
            let da = (a.gain - gain).powi(2) + (a.treble - treble).powi(2);
            let db = (b.gain - gain).powi(2) + (b.treble - treble).powi(2);
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

