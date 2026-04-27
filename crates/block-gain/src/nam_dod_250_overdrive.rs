use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_dod_250_overdrive";
pub const DISPLAY_NAME: &str = "DOD 250 Overdrive";
const BRAND: &str = "dod";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "0_l_10_ttsv10",  model_path: "pedals/dod_250_overdrive/dod_250_g_0_l_10_ttsv10.nam" },
    NamCapture { tone: "10_l_10_ttsv10", model_path: "pedals/dod_250_overdrive/dod_250_g_10_l_10_ttsv10.nam" },
    NamCapture { tone: "10_l_8_ttsv10",  model_path: "pedals/dod_250_overdrive/dod_250_g_10_l_8_ttsv10.nam" },
    NamCapture { tone: "2_l_10_ttsv10",  model_path: "pedals/dod_250_overdrive/dod_250_g_2_l_10_ttsv10.nam" },
    NamCapture { tone: "2_l_8_ttsv10",   model_path: "pedals/dod_250_overdrive/dod_250_g_2_l_8_ttsv10.nam" },
    NamCapture { tone: "3_l_10_ttsv10",  model_path: "pedals/dod_250_overdrive/dod_250_g_3_l_10_ttsv10.nam" },
    NamCapture { tone: "3_l_8_ttsv10",   model_path: "pedals/dod_250_overdrive/dod_250_g_3_l_8_ttsv10.nam" },
    NamCapture { tone: "4_l_10_ttsv10",  model_path: "pedals/dod_250_overdrive/dod_250_g_4_l_10_ttsv10.nam" },
    NamCapture { tone: "4_l_8_ttsv10",   model_path: "pedals/dod_250_overdrive/dod_250_g_4_l_8_ttsv10.nam" },
    NamCapture { tone: "5_l_10_ttsv10",  model_path: "pedals/dod_250_overdrive/dod_250_g_5_l_10_ttsv10.nam" },
    NamCapture { tone: "5_l_8_ttsv10",   model_path: "pedals/dod_250_overdrive/dod_250_g_5_l_8_ttsv10.nam" },
    NamCapture { tone: "6_l_10_ttsv10",  model_path: "pedals/dod_250_overdrive/dod_250_g_6_l_10_ttsv10.nam" },
    NamCapture { tone: "6_l_8_ttsv10",   model_path: "pedals/dod_250_overdrive/dod_250_g_6_l_8_ttsv10.nam" },
    NamCapture { tone: "7_l_10_ttsv10",  model_path: "pedals/dod_250_overdrive/dod_250_g_7_l_10_ttsv10.nam" },
    NamCapture { tone: "7_l_8_ttsv10",   model_path: "pedals/dod_250_overdrive/dod_250_g_7_l_8_ttsv10.nam" },
    NamCapture { tone: "8_l_10_ttsv10",  model_path: "pedals/dod_250_overdrive/dod_250_g_8_l_10_ttsv10.nam" },
    NamCapture { tone: "8_l_8_ttsv10",   model_path: "pedals/dod_250_overdrive/dod_250_g_8_l_8_ttsv10.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("0_l_10_ttsv10"),
        &[
            ("0_l_10_ttsv10",  "0 L 10 Ttsv10"),
            ("10_l_10_ttsv10", "10 L 10 Ttsv10"),
            ("10_l_8_ttsv10",  "10 L 8 Ttsv10"),
            ("2_l_10_ttsv10",  "2 L 10 Ttsv10"),
            ("2_l_8_ttsv10",   "2 L 8 Ttsv10"),
            ("3_l_10_ttsv10",  "3 L 10 Ttsv10"),
            ("3_l_8_ttsv10",   "3 L 8 Ttsv10"),
            ("4_l_10_ttsv10",  "4 L 10 Ttsv10"),
            ("4_l_8_ttsv10",   "4 L 8 Ttsv10"),
            ("5_l_10_ttsv10",  "5 L 10 Ttsv10"),
            ("5_l_8_ttsv10",   "5 L 8 Ttsv10"),
            ("6_l_10_ttsv10",  "6 L 10 Ttsv10"),
            ("6_l_8_ttsv10",   "6 L 8 Ttsv10"),
            ("7_l_10_ttsv10",  "7 L 10 Ttsv10"),
            ("7_l_8_ttsv10",   "7 L 8 Ttsv10"),
            ("8_l_10_ttsv10",  "8 L 10 Ttsv10"),
            ("8_l_8_ttsv10",   "8 L 8 Ttsv10"),
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
