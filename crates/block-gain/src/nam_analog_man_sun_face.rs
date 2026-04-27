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

pub const MODEL_ID: &str = "nam_analog_man_sun_face";
pub const DISPLAY_NAME: &str = "Analog Man Sun Face";
const BRAND: &str = "analogman";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    compression: f32,
    filter: f32,
    volume: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Standard,
}

const COMPRESSION_MIN: f32 = 3.0;
const COMPRESSION_MAX: f32 = 10.0;
const FILTER_MIN: f32 = 9.0;
const FILTER_MAX: f32 = 10.0;
const VOLUME_MIN: f32 = 7.0;
const VOLUME_MAX: f32 = 8.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { compression: 10.0, filter: 10.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/analog_man_sun_face/sunface_bc183_v7_f10_c10.nam" },
    GridCapture { compression: 3.0, filter: 10.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/analog_man_sun_face/sunface_bc183_v7_f10_c3.nam" },
    GridCapture { compression: 5.0, filter: 10.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/analog_man_sun_face/sunface_bc183_v7_f10_c5.nam" },
    GridCapture { compression: 8.0, filter: 10.0, volume: 7.0, size: NamSize::Standard, model_path: "pedals/analog_man_sun_face/sunface_bc183_v7_f10_c8.nam" },
    GridCapture { compression: 10.0, filter: 9.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/analog_man_sun_face/sunface_bc183_v8_f9_c10_cleanup.nam" },
    GridCapture { compression: 6.0, filter: 9.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/analog_man_sun_face/sunface_bc183_v8_f9_c6_cleanup.nam" },
    GridCapture { compression: 9.0, filter: 9.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/analog_man_sun_face/sunface_bc183_v8_f9_c9_cleanup.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("compression", "Compression", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("filter", "Filter", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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
    let compression_pct = required_f32(params, "compression").map_err(anyhow::Error::msg)?;
    let filter_pct = required_f32(params, "filter").map_err(anyhow::Error::msg)?;
    let volume_pct = required_f32(params, "volume").map_err(anyhow::Error::msg)?;
    let compression = COMPRESSION_MIN + (compression_pct / 100.0) * (COMPRESSION_MAX - COMPRESSION_MIN);
    let filter = FILTER_MIN + (filter_pct / 100.0) * (FILTER_MAX - FILTER_MIN);
    let volume = VOLUME_MIN + (volume_pct / 100.0) * (VOLUME_MAX - VOLUME_MIN);
    let _size = NamSize::Standard;
    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::Standard);
    candidates
        .min_by(|a, b| {
            let da = (a.compression - compression).powi(2) + (a.filter - filter).powi(2) + (a.volume - volume).powi(2);
            let db = (b.compression - compression).powi(2) + (b.filter - filter).powi(2) + (b.volume - volume).powi(2);
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

