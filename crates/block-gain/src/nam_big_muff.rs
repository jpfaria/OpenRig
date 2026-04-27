use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_big_muff";
pub const DISPLAY_NAME: &str = "Big Muff";
const BRAND: &str = "ehx";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "2_s_0",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_0.nam" },
    NamCapture { tone: "2_s_0_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_0_feather.nam" },
    NamCapture { tone: "2_s_10",           model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_10.nam" },
    NamCapture { tone: "2_s_10_feather",   model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_10_feather.nam" },
    NamCapture { tone: "2_s_2",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_2.nam" },
    NamCapture { tone: "2_s_2_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_2_feather.nam" },
    NamCapture { tone: "2_s_5",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_5.nam" },
    NamCapture { tone: "2_s_5_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_5_feather.nam" },
    NamCapture { tone: "2_s_8",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_8.nam" },
    NamCapture { tone: "2_s_8_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_8_feather.nam" },
    NamCapture { tone: "3_s_0",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_0.nam" },
    NamCapture { tone: "3_s_0_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_0_feather.nam" },
    NamCapture { tone: "3_s_10",           model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_10.nam" },
    NamCapture { tone: "3_s_10_feather",   model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_10_feather.nam" },
    NamCapture { tone: "3_s_2",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_2.nam" },
    NamCapture { tone: "3_s_2_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_2_feather.nam" },
    NamCapture { tone: "3_s_5",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_5.nam" },
    NamCapture { tone: "3_s_5_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_5_feather.nam" },
    NamCapture { tone: "3_s_8",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_8.nam" },
    NamCapture { tone: "3_s_8_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_8_feather.nam" },
    NamCapture { tone: "4_s_0",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_0.nam" },
    NamCapture { tone: "4_s_0_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_0_feather.nam" },
    NamCapture { tone: "4_s_10",           model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_10.nam" },
    NamCapture { tone: "4_s_10_feather",   model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_10_feather.nam" },
    NamCapture { tone: "4_s_2",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_2.nam" },
    NamCapture { tone: "4_s_2_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_2_feather.nam" },
    NamCapture { tone: "4_s_5",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_5.nam" },
    NamCapture { tone: "4_s_5_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_5_feather.nam" },
    NamCapture { tone: "4_s_8",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_8.nam" },
    NamCapture { tone: "4_s_8_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_8_feather.nam" },
    NamCapture { tone: "5_s_0",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_0.nam" },
    NamCapture { tone: "5_s_0_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_0_feather.nam" },
    NamCapture { tone: "5_s_10",           model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_10.nam" },
    NamCapture { tone: "5_s_10_feather",   model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_10_feather.nam" },
    NamCapture { tone: "5_s_2",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_2.nam" },
    NamCapture { tone: "5_s_2_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_2_feather.nam" },
    NamCapture { tone: "5_s_5",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_5.nam" },
    NamCapture { tone: "5_s_5_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_5_feather.nam" },
    NamCapture { tone: "5_s_8",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_8.nam" },
    NamCapture { tone: "5_s_8_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_8_feather.nam" },
    NamCapture { tone: "6_s_0",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_0.nam" },
    NamCapture { tone: "6_s_0_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_0_feather.nam" },
    NamCapture { tone: "6_s_10",           model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_10.nam" },
    NamCapture { tone: "6_s_10_feather",   model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_10_feather.nam" },
    NamCapture { tone: "6_s_2",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_2.nam" },
    NamCapture { tone: "6_s_2_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_2_feather.nam" },
    NamCapture { tone: "6_s_5",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_5.nam" },
    NamCapture { tone: "6_s_5_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_5_feather.nam" },
    NamCapture { tone: "6_s_8",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_8.nam" },
    NamCapture { tone: "6_s_8_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_8_feather.nam" },
    NamCapture { tone: "7_s_0",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_0.nam" },
    NamCapture { tone: "7_s_0_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_0_feather.nam" },
    NamCapture { tone: "7_s_10",           model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_10.nam" },
    NamCapture { tone: "7_s_10_feather",   model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_10_feather.nam" },
    NamCapture { tone: "7_s_2",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_2.nam" },
    NamCapture { tone: "7_s_2_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_2_feather.nam" },
    NamCapture { tone: "7_s_5",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_5.nam" },
    NamCapture { tone: "7_s_5_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_5_feather.nam" },
    NamCapture { tone: "7_s_8",            model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_8.nam" },
    NamCapture { tone: "7_s_8_feather",    model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_8_feather.nam" },
    NamCapture { tone: "byp_s_0",          model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_0.nam" },
    NamCapture { tone: "byp_s_0_feather",  model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_0_feather.nam" },
    NamCapture { tone: "byp_s_10",         model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_10.nam" },
    NamCapture { tone: "byp_s_10_feather", model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_10_feather.nam" },
    NamCapture { tone: "byp_s_2",          model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_2.nam" },
    NamCapture { tone: "byp_s_2_feather",  model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_2_feather.nam" },
    NamCapture { tone: "byp_s_5",          model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_5.nam" },
    NamCapture { tone: "byp_s_5_feather",  model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_5_feather.nam" },
    NamCapture { tone: "byp_s_8",          model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_8.nam" },
    NamCapture { tone: "byp_s_8_feather",  model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_8_feather.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("2_s_0"),
        &[
            ("2_s_0",            "2 S 0"),
            ("2_s_0_feather",    "2 S 0 Feather"),
            ("2_s_10",           "2 S 10"),
            ("2_s_10_feather",   "2 S 10 Feather"),
            ("2_s_2",            "2 S 2"),
            ("2_s_2_feather",    "2 S 2 Feather"),
            ("2_s_5",            "2 S 5"),
            ("2_s_5_feather",    "2 S 5 Feather"),
            ("2_s_8",            "2 S 8"),
            ("2_s_8_feather",    "2 S 8 Feather"),
            ("3_s_0",            "3 S 0"),
            ("3_s_0_feather",    "3 S 0 Feather"),
            ("3_s_10",           "3 S 10"),
            ("3_s_10_feather",   "3 S 10 Feather"),
            ("3_s_2",            "3 S 2"),
            ("3_s_2_feather",    "3 S 2 Feather"),
            ("3_s_5",            "3 S 5"),
            ("3_s_5_feather",    "3 S 5 Feather"),
            ("3_s_8",            "3 S 8"),
            ("3_s_8_feather",    "3 S 8 Feather"),
            ("4_s_0",            "4 S 0"),
            ("4_s_0_feather",    "4 S 0 Feather"),
            ("4_s_10",           "4 S 10"),
            ("4_s_10_feather",   "4 S 10 Feather"),
            ("4_s_2",            "4 S 2"),
            ("4_s_2_feather",    "4 S 2 Feather"),
            ("4_s_5",            "4 S 5"),
            ("4_s_5_feather",    "4 S 5 Feather"),
            ("4_s_8",            "4 S 8"),
            ("4_s_8_feather",    "4 S 8 Feather"),
            ("5_s_0",            "5 S 0"),
            ("5_s_0_feather",    "5 S 0 Feather"),
            ("5_s_10",           "5 S 10"),
            ("5_s_10_feather",   "5 S 10 Feather"),
            ("5_s_2",            "5 S 2"),
            ("5_s_2_feather",    "5 S 2 Feather"),
            ("5_s_5",            "5 S 5"),
            ("5_s_5_feather",    "5 S 5 Feather"),
            ("5_s_8",            "5 S 8"),
            ("5_s_8_feather",    "5 S 8 Feather"),
            ("6_s_0",            "6 S 0"),
            ("6_s_0_feather",    "6 S 0 Feather"),
            ("6_s_10",           "6 S 10"),
            ("6_s_10_feather",   "6 S 10 Feather"),
            ("6_s_2",            "6 S 2"),
            ("6_s_2_feather",    "6 S 2 Feather"),
            ("6_s_5",            "6 S 5"),
            ("6_s_5_feather",    "6 S 5 Feather"),
            ("6_s_8",            "6 S 8"),
            ("6_s_8_feather",    "6 S 8 Feather"),
            ("7_s_0",            "7 S 0"),
            ("7_s_0_feather",    "7 S 0 Feather"),
            ("7_s_10",           "7 S 10"),
            ("7_s_10_feather",   "7 S 10 Feather"),
            ("7_s_2",            "7 S 2"),
            ("7_s_2_feather",    "7 S 2 Feather"),
            ("7_s_5",            "7 S 5"),
            ("7_s_5_feather",    "7 S 5 Feather"),
            ("7_s_8",            "7 S 8"),
            ("7_s_8_feather",    "7 S 8 Feather"),
            ("byp_s_0",          "Byp S 0"),
            ("byp_s_0_feather",  "Byp S 0 Feather"),
            ("byp_s_10",         "Byp S 10"),
            ("byp_s_10_feather", "Byp S 10 Feather"),
            ("byp_s_2",          "Byp S 2"),
            ("byp_s_2_feather",  "Byp S 2 Feather"),
            ("byp_s_5",          "Byp S 5"),
            ("byp_s_5_feather",  "Byp S 5 Feather"),
            ("byp_s_8",          "Byp S 8"),
            ("byp_s_8_feather",  "Byp S 8 Feather"),
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
