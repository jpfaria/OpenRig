use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_paul_cochrane_timmy";
pub const DISPLAY_NAME: &str = "Paul Cochrane Timmy";
const BRAND: &str = "cochrane";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "12_00_t_12_00_g_10_00_c_c_ttsv1", model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_12_00_t_12_00_g_10_00_c_c_ttsv1.nam" },
    NamCapture { tone: "12_00_t_12_00_g_10_00_c_l_ttsv1", model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_12_00_t_12_00_g_10_00_c_l_ttsv1.nam" },
    NamCapture { tone: "12_00_t_12_00_g_10_00_c_r_ttsv1", model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_12_00_t_12_00_g_10_00_c_r_ttsv1.nam" },
    NamCapture { tone: "12_00_t_12_00_g_12_00_c_c_ttsv1", model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_12_00_t_12_00_g_12_00_c_c_ttsv1.nam" },
    NamCapture { tone: "12_00_t_12_00_g_12_00_c_l_ttsv1", model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_12_00_t_12_00_g_12_00_c_l_ttsv1.nam" },
    NamCapture { tone: "12_00_t_12_00_g_12_00_c_r_ttsv1", model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_12_00_t_12_00_g_12_00_c_r_ttsv1.nam" },
    NamCapture { tone: "12_00_t_12_00_g_2_00_c_c_ttsv10", model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_12_00_t_12_00_g_2_00_c_c_ttsv10.nam" },
    NamCapture { tone: "12_00_t_12_00_g_2_00_c_l_ttsv10", model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_12_00_t_12_00_g_2_00_c_l_ttsv10.nam" },
    NamCapture { tone: "12_00_t_12_00_g_2_00_c_r_ttsv10", model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_12_00_t_12_00_g_2_00_c_r_ttsv10.nam" },
    NamCapture { tone: "1_00_t_2_00_g_10_00_c_c_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_1_00_t_2_00_g_10_00_c_c_ttsv10.nam" },
    NamCapture { tone: "1_00_t_2_00_g_10_00_c_l_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_1_00_t_2_00_g_10_00_c_l_ttsv10.nam" },
    NamCapture { tone: "1_00_t_2_00_g_10_00_c_r_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_1_00_t_2_00_g_10_00_c_r_ttsv10.nam" },
    NamCapture { tone: "1_00_t_2_00_g_12_00_c_c_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_1_00_t_2_00_g_12_00_c_c_ttsv10.nam" },
    NamCapture { tone: "1_00_t_2_00_g_12_00_c_l_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_1_00_t_2_00_g_12_00_c_l_ttsv10.nam" },
    NamCapture { tone: "1_00_t_2_00_g_12_00_c_r_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_1_00_t_2_00_g_12_00_c_r_ttsv10.nam" },
    NamCapture { tone: "1_00_t_2_00_g_2_00_c_c_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_1_00_t_2_00_g_2_00_c_c_ttsv10.nam" },
    NamCapture { tone: "1_00_t_2_00_g_2_00_c_l_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_1_00_t_2_00_g_2_00_c_l_ttsv10.nam" },
    NamCapture { tone: "1_00_t_2_00_g_2_00_c_r_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_1_00_t_2_00_g_2_00_c_r_ttsv10.nam" },
    NamCapture { tone: "2_00_t_3_00_g_10_00_c_c_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_2_00_t_3_00_g_10_00_c_c_ttsv10.nam" },
    NamCapture { tone: "2_00_t_3_00_g_10_00_c_l_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_2_00_t_3_00_g_10_00_c_l_ttsv10.nam" },
    NamCapture { tone: "2_00_t_3_00_g_10_00_c_r_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_2_00_t_3_00_g_10_00_c_r_ttsv10.nam" },
    NamCapture { tone: "2_00_t_3_00_g_12_00_c_c_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_2_00_t_3_00_g_12_00_c_c_ttsv10.nam" },
    NamCapture { tone: "2_00_t_3_00_g_12_00_c_l_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_2_00_t_3_00_g_12_00_c_l_ttsv10.nam" },
    NamCapture { tone: "2_00_t_3_00_g_12_00_c_r_ttsv10",  model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_2_00_t_3_00_g_12_00_c_r_ttsv10.nam" },
    NamCapture { tone: "2_00_t_3_00_g_2_00_c_c_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_2_00_t_3_00_g_2_00_c_c_ttsv10.nam" },
    NamCapture { tone: "2_00_t_3_00_g_2_00_c_l_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_2_00_t_3_00_g_2_00_c_l_ttsv10.nam" },
    NamCapture { tone: "2_00_t_3_00_g_2_00_c_r_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_2_00_t_3_00_g_2_00_c_r_ttsv10.nam" },
    NamCapture { tone: "3_00_t_max_g_10_00_c_c_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_3_00_t_max_g_10_00_c_c_ttsv10.nam" },
    NamCapture { tone: "3_00_t_max_g_10_00_c_l_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_3_00_t_max_g_10_00_c_l_ttsv10.nam" },
    NamCapture { tone: "3_00_t_max_g_10_00_c_r_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_3_00_t_max_g_10_00_c_r_ttsv10.nam" },
    NamCapture { tone: "3_00_t_max_g_12_00_c_c_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_3_00_t_max_g_12_00_c_c_ttsv10.nam" },
    NamCapture { tone: "3_00_t_max_g_12_00_c_l_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_3_00_t_max_g_12_00_c_l_ttsv10.nam" },
    NamCapture { tone: "3_00_t_max_g_12_00_c_r_ttsv10",   model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_3_00_t_max_g_12_00_c_r_ttsv10.nam" },
    NamCapture { tone: "3_00_t_max_g_2_00_c_c_ttsv10",    model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_3_00_t_max_g_2_00_c_c_ttsv10.nam" },
    NamCapture { tone: "3_00_t_max_g_2_00_c_l_ttsv10",    model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_3_00_t_max_g_2_00_c_l_ttsv10.nam" },
    NamCapture { tone: "3_00_t_max_g_2_00_c_r_ttsv10",    model_path: "pedals/paul_cochrane_timmy/mxr_timmy_v_1_00_b_3_00_t_max_g_2_00_c_r_ttsv10.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("12_00_t_12_00_g_10_00_c_c_ttsv1"),
        &[
            ("12_00_t_12_00_g_10_00_c_c_ttsv1", "12 00 T 12 00 G 10 00 C C Ttsv1"),
            ("12_00_t_12_00_g_10_00_c_l_ttsv1", "12 00 T 12 00 G 10 00 C L Ttsv1"),
            ("12_00_t_12_00_g_10_00_c_r_ttsv1", "12 00 T 12 00 G 10 00 C R Ttsv1"),
            ("12_00_t_12_00_g_12_00_c_c_ttsv1", "12 00 T 12 00 G 12 00 C C Ttsv1"),
            ("12_00_t_12_00_g_12_00_c_l_ttsv1", "12 00 T 12 00 G 12 00 C L Ttsv1"),
            ("12_00_t_12_00_g_12_00_c_r_ttsv1", "12 00 T 12 00 G 12 00 C R Ttsv1"),
            ("12_00_t_12_00_g_2_00_c_c_ttsv10", "12 00 T 12 00 G 2 00 C C Ttsv10"),
            ("12_00_t_12_00_g_2_00_c_l_ttsv10", "12 00 T 12 00 G 2 00 C L Ttsv10"),
            ("12_00_t_12_00_g_2_00_c_r_ttsv10", "12 00 T 12 00 G 2 00 C R Ttsv10"),
            ("1_00_t_2_00_g_10_00_c_c_ttsv10",  "1 00 T 2 00 G 10 00 C C Ttsv10"),
            ("1_00_t_2_00_g_10_00_c_l_ttsv10",  "1 00 T 2 00 G 10 00 C L Ttsv10"),
            ("1_00_t_2_00_g_10_00_c_r_ttsv10",  "1 00 T 2 00 G 10 00 C R Ttsv10"),
            ("1_00_t_2_00_g_12_00_c_c_ttsv10",  "1 00 T 2 00 G 12 00 C C Ttsv10"),
            ("1_00_t_2_00_g_12_00_c_l_ttsv10",  "1 00 T 2 00 G 12 00 C L Ttsv10"),
            ("1_00_t_2_00_g_12_00_c_r_ttsv10",  "1 00 T 2 00 G 12 00 C R Ttsv10"),
            ("1_00_t_2_00_g_2_00_c_c_ttsv10",   "1 00 T 2 00 G 2 00 C C Ttsv10"),
            ("1_00_t_2_00_g_2_00_c_l_ttsv10",   "1 00 T 2 00 G 2 00 C L Ttsv10"),
            ("1_00_t_2_00_g_2_00_c_r_ttsv10",   "1 00 T 2 00 G 2 00 C R Ttsv10"),
            ("2_00_t_3_00_g_10_00_c_c_ttsv10",  "2 00 T 3 00 G 10 00 C C Ttsv10"),
            ("2_00_t_3_00_g_10_00_c_l_ttsv10",  "2 00 T 3 00 G 10 00 C L Ttsv10"),
            ("2_00_t_3_00_g_10_00_c_r_ttsv10",  "2 00 T 3 00 G 10 00 C R Ttsv10"),
            ("2_00_t_3_00_g_12_00_c_c_ttsv10",  "2 00 T 3 00 G 12 00 C C Ttsv10"),
            ("2_00_t_3_00_g_12_00_c_l_ttsv10",  "2 00 T 3 00 G 12 00 C L Ttsv10"),
            ("2_00_t_3_00_g_12_00_c_r_ttsv10",  "2 00 T 3 00 G 12 00 C R Ttsv10"),
            ("2_00_t_3_00_g_2_00_c_c_ttsv10",   "2 00 T 3 00 G 2 00 C C Ttsv10"),
            ("2_00_t_3_00_g_2_00_c_l_ttsv10",   "2 00 T 3 00 G 2 00 C L Ttsv10"),
            ("2_00_t_3_00_g_2_00_c_r_ttsv10",   "2 00 T 3 00 G 2 00 C R Ttsv10"),
            ("3_00_t_max_g_10_00_c_c_ttsv10",   "3 00 T Max G 10 00 C C Ttsv10"),
            ("3_00_t_max_g_10_00_c_l_ttsv10",   "3 00 T Max G 10 00 C L Ttsv10"),
            ("3_00_t_max_g_10_00_c_r_ttsv10",   "3 00 T Max G 10 00 C R Ttsv10"),
            ("3_00_t_max_g_12_00_c_c_ttsv10",   "3 00 T Max G 12 00 C C Ttsv10"),
            ("3_00_t_max_g_12_00_c_l_ttsv10",   "3 00 T Max G 12 00 C L Ttsv10"),
            ("3_00_t_max_g_12_00_c_r_ttsv10",   "3 00 T Max G 12 00 C R Ttsv10"),
            ("3_00_t_max_g_2_00_c_c_ttsv10",    "3 00 T Max G 2 00 C C Ttsv10"),
            ("3_00_t_max_g_2_00_c_l_ttsv10",    "3 00 T Max G 2 00 C L Ttsv10"),
            ("3_00_t_max_g_2_00_c_r_ttsv10",    "3 00 T Max G 2 00 C R Ttsv10"),
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
