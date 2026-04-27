use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fulltone_69";
pub const DISPLAY_NAME: &str = "Fulltone 69";
const BRAND: &str = "fulltone";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "69_hendrix_67_vol_2_contour_12_bias_and_",   model_path: "pedals/fulltone_69/69_hendrix_67_vol_2_contour_12_bias_and_fuzz_full.nam" },
    NamCapture { tone: "69_high_contour_vol_2_contour_bias_and_f",   model_path: "pedals/fulltone_69/69_high_contour_vol_2_contour_bias_and_fuzz_full.nam" },
    NamCapture { tone: "69_high_vol_low_contour_contour_off_vol_",   model_path: "pedals/fulltone_69/69_high_vol_low_contour_contour_off_vol_bias_and_f.nam" },
    NamCapture { tone: "69_high_vol_mid_contour_contour_12_vol_b",   model_path: "pedals/fulltone_69/69_high_vol_mid_contour_contour_12_vol_bias_and_fu.nam" },
    NamCapture { tone: "69_high_vol_mid_contour_contour_12_vol_b_1", model_path: "pedals/fulltone_69/69_high_vol_mid_contour_contour_12_vol_bias_and_fu_1.nam" },
    NamCapture { tone: "69_maxx_fuzz_vol_contour_bias_and_fuzz_f",   model_path: "pedals/fulltone_69/69_maxx_fuzz_vol_contour_bias_and_fuzz_full_gain_t.nam" },
    NamCapture { tone: "69_stock_fuzzface_vol_2_contour_off_bias",   model_path: "pedals/fulltone_69/69_stock_fuzzface_vol_2_contour_off_bias_and_fuzz_.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("69_hendrix_67_vol_2_contour_12_bias_and_"),
        &[
            ("69_hendrix_67_vol_2_contour_12_bias_and_",   "69 Hendrix 67 Vol 2 Contour 12 Bias And Fuzz Full"),
            ("69_high_contour_vol_2_contour_bias_and_f",   "69 High Contour Vol 2 Contour Bias And Fuzz Full"),
            ("69_high_vol_low_contour_contour_off_vol_",   "69 High Vol Low Contour Contour Off Vol Bias And F"),
            ("69_high_vol_mid_contour_contour_12_vol_b",   "69 High Vol Mid Contour Contour 12 Vol Bias And Fu"),
            ("69_high_vol_mid_contour_contour_12_vol_b_1", "69 High Vol Mid Contour Contour 12 Vol Bias And Fu 1"),
            ("69_maxx_fuzz_vol_contour_bias_and_fuzz_f",   "69 Maxx Fuzz Vol Contour Bias And Fuzz Full Gain T"),
            ("69_stock_fuzzface_vol_2_contour_off_bias",   "69 Stock Fuzzface Vol 2 Contour Off Bias And Fuzz "),
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
