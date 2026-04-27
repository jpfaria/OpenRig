use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mxr_classic_108_fuzz";
pub const DISPLAY_NAME: &str = "MXR Classic 108 Fuzz";
const BRAND: &str = "mxr";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "10_00_buffer_ttsv10", model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_10_00_buffer_ttsv10.nam" },
    NamCapture { tone: "10_00_ttsv10",        model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_10_00_ttsv10.nam" },
    NamCapture { tone: "11_00_buffer_ttsv10", model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_11_00_buffer_ttsv10.nam" },
    NamCapture { tone: "11_00_ttsv10",        model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_11_00_ttsv10.nam" },
    NamCapture { tone: "12_00_buffer_ttsv10", model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_12_00_buffer_ttsv10.nam" },
    NamCapture { tone: "12_00_ttsv10",        model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_12_00_ttsv10.nam" },
    NamCapture { tone: "1_00_buffer_ttsv10",  model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_1_00_buffer_ttsv10.nam" },
    NamCapture { tone: "1_00_ttsv10",         model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_1_00_ttsv10.nam" },
    NamCapture { tone: "2_00_buffer_ttsv10",  model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_2_00_buffer_ttsv10.nam" },
    NamCapture { tone: "2_00_ttsv10",         model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_2_00_ttsv10.nam" },
    NamCapture { tone: "3_00_buffer_ttsv10",  model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_3_00_buffer_ttsv10.nam" },
    NamCapture { tone: "3_00_ttsv10",         model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_3_00_ttsv10.nam" },
    NamCapture { tone: "4_00_buffer_ttsv10",  model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_4_00_buffer_ttsv10.nam" },
    NamCapture { tone: "4_00_ttsv10",         model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_4_00_ttsv10.nam" },
    NamCapture { tone: "9_00_buffer_ttsv10",  model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_9_00_buffer_ttsv10.nam" },
    NamCapture { tone: "9_00_ttsv10",         model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_9_00_ttsv10.nam" },
    NamCapture { tone: "max_buffer_ttsv10",   model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_max_buffer_ttsv10.nam" },
    NamCapture { tone: "max_ttsv10",          model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_max_ttsv10.nam" },
    NamCapture { tone: "min_buffer_ttsv10",   model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_min_buffer_ttsv10.nam" },
    NamCapture { tone: "min_ttsv10",          model_path: "pedals/mxr_classic_108_fuzz/mxr_108_fuzz_v_max_f_min_ttsv10.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("10_00_buffer_ttsv10"),
        &[
            ("10_00_buffer_ttsv10", "10 00 Buffer Ttsv10"),
            ("10_00_ttsv10",        "10 00 Ttsv10"),
            ("11_00_buffer_ttsv10", "11 00 Buffer Ttsv10"),
            ("11_00_ttsv10",        "11 00 Ttsv10"),
            ("12_00_buffer_ttsv10", "12 00 Buffer Ttsv10"),
            ("12_00_ttsv10",        "12 00 Ttsv10"),
            ("1_00_buffer_ttsv10",  "1 00 Buffer Ttsv10"),
            ("1_00_ttsv10",         "1 00 Ttsv10"),
            ("2_00_buffer_ttsv10",  "2 00 Buffer Ttsv10"),
            ("2_00_ttsv10",         "2 00 Ttsv10"),
            ("3_00_buffer_ttsv10",  "3 00 Buffer Ttsv10"),
            ("3_00_ttsv10",         "3 00 Ttsv10"),
            ("4_00_buffer_ttsv10",  "4 00 Buffer Ttsv10"),
            ("4_00_ttsv10",         "4 00 Ttsv10"),
            ("9_00_buffer_ttsv10",  "9 00 Buffer Ttsv10"),
            ("9_00_ttsv10",         "9 00 Ttsv10"),
            ("max_buffer_ttsv10",   "Max Buffer Ttsv10"),
            ("max_ttsv10",          "Max Ttsv10"),
            ("min_buffer_ttsv10",   "Min Buffer Ttsv10"),
            ("min_ttsv10",          "Min Ttsv10"),
        ],
    )];
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static NamCapture> {
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.tone == tone)
        .ok_or_else(|| anyhow!("gain model '{}' does not support tone='{}'", MODEL_ID, tone))
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
