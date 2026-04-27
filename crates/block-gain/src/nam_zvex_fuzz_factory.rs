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

pub const MODEL_ID: &str = "nam_zvex_fuzz_factory";
pub const DISPLAY_NAME: &str = "ZVEX Fuzz Factory";
const BRAND: &str = "zvex";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    compression: f32,
    filter: f32,
    gain: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Standard,
}

const COMPRESSION_MIN: f32 = 3.0;
const COMPRESSION_MAX: f32 = 9.0;
const FILTER_MIN: f32 = 3.0;
const FILTER_MAX: f32 = 12.0;
const GAIN_MIN: f32 = 3.0;
const GAIN_MAX: f32 = 12.3;

const CAPTURES: &[GridCapture] = &[
    GridCapture { compression: 3.0, filter: 12.0, gain: 12.0, size: NamSize::Standard, model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_12_00_c_3_00_f_12_00_.nam" },
    GridCapture { compression: 3.0, filter: 3.0, gain: 12.0, size: NamSize::Standard, model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_12_00_c_3_00_f_min_s_.nam" },
    GridCapture { compression: 3.0, filter: 3.0, gain: 12.3, size: NamSize::Standard, model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_12_30_c_min_f_min_s_m.nam" },
    GridCapture { compression: 9.0, filter: 3.0, gain: 3.0, size: NamSize::Standard, model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_3_00_c_9_00_f_3_00_s_.nam" },
    GridCapture { compression: 4.0, filter: 9.0, gain: 9.0, size: NamSize::Standard, model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_9_00_c_4_00_f_9_00_s_.nam" },
    GridCapture { compression: 4.0, filter: 12.0, gain: 3.0, size: NamSize::Standard, model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_min_c_4_00_f_max_s_2_.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("compression", "Compression", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("filter", "Filter", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("gain", "Gain", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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
    let compression_pct = required_f32(params, "compression").map_err(anyhow::Error::msg)?;
    let filter_pct = required_f32(params, "filter").map_err(anyhow::Error::msg)?;
    let gain_pct = required_f32(params, "gain").map_err(anyhow::Error::msg)?;
    let compression = COMPRESSION_MIN + (compression_pct / 100.0) * (COMPRESSION_MAX - COMPRESSION_MIN);
    let filter = FILTER_MIN + (filter_pct / 100.0) * (FILTER_MAX - FILTER_MIN);
    let gain = GAIN_MIN + (gain_pct / 100.0) * (GAIN_MAX - GAIN_MIN);
    let _size = NamSize::Standard;
    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::Standard);
    candidates
        .min_by(|a, b| {
            let da = (a.compression - compression).powi(2) + (a.filter - filter).powi(2) + (a.gain - gain).powi(2);
            let db = (b.compression - compression).powi(2) + (b.filter - filter).powi(2) + (b.gain - gain).powi(2);
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

