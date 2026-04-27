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

pub const MODEL_ID: &str = "nam_mxr_classic_108_fuzz";
pub const DISPLAY_NAME: &str = "MXR Classic 108 Fuzz";
const BRAND: &str = "mxr";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    filter: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Standard,
}

const FILTER_MIN: f32 = 1.0;
const FILTER_MAX: f32 = 12.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { filter: 10.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_10_00_buffer_ttsv10.nam" },
    GridCapture { filter: 10.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_10_00_ttsv10.nam" },
    GridCapture { filter: 11.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_11_00_buffer_ttsv10.nam" },
    GridCapture { filter: 11.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_11_00_ttsv10.nam" },
    GridCapture { filter: 12.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_12_00_buffer_ttsv10.nam" },
    GridCapture { filter: 12.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_12_00_ttsv10.nam" },
    GridCapture { filter: 1.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_1_00_buffer_ttsv10.nam" },
    GridCapture { filter: 1.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_1_00_ttsv10.nam" },
    GridCapture { filter: 2.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_2_00_buffer_ttsv10.nam" },
    GridCapture { filter: 2.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_2_00_ttsv10.nam" },
    GridCapture { filter: 3.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_3_00_buffer_ttsv10.nam" },
    GridCapture { filter: 3.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_3_00_ttsv10.nam" },
    GridCapture { filter: 4.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_4_00_buffer_ttsv10.nam" },
    GridCapture { filter: 4.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_4_00_ttsv10.nam" },
    GridCapture { filter: 9.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_9_00_buffer_ttsv10.nam" },
    GridCapture { filter: 9.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_9_00_ttsv10.nam" },
    GridCapture { filter: 12.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_max_buffer_ttsv10.nam" },
    GridCapture { filter: 12.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_max_ttsv10.nam" },
    GridCapture { filter: 1.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_min_buffer_ttsv10.nam" },
    GridCapture { filter: 1.0, size: NamSize::Standard, model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_min_ttsv10.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("filter", "Filter", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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
    let filter_pct = required_f32(params, "filter").map_err(anyhow::Error::msg)?;
    let filter = FILTER_MIN + (filter_pct / 100.0) * (FILTER_MAX - FILTER_MIN);
    let _size = NamSize::Standard;
    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::Standard);
    candidates
        .min_by(|a, b| {
            let da = (a.filter - filter).powi(2);
            let db = (b.filter - filter).powi(2);
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

