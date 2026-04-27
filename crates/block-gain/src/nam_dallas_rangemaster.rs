use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_dallas_rangemaster";
pub const DISPLAY_NAME: &str = "Dallas Rangemaster";
const BRAND: &str = "dallas";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "v10_t0_c",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t0_c.nam" },
    NamCapture { tone: "v10_t0_s",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t0_s.nam" },
    NamCapture { tone: "v10_t0_xs",       model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t0_xs.nam" },
    NamCapture { tone: "v10_t10_main_c",  model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t10_main_c.nam" },
    NamCapture { tone: "v10_t10_main_s",  model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t10_main_s.nam" },
    NamCapture { tone: "v10_t10_main_xs", model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t10_main_xs.nam" },
    NamCapture { tone: "v10_t3_c",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t3_c.nam" },
    NamCapture { tone: "v10_t3_s",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t3_s.nam" },
    NamCapture { tone: "v10_t3_xs",       model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t3_xs.nam" },
    NamCapture { tone: "v10_t5_c",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t5_c.nam" },
    NamCapture { tone: "v10_t5_s",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t5_s.nam" },
    NamCapture { tone: "v10_t5_xs",       model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t5_xs.nam" },
    NamCapture { tone: "v10_t7_c",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t7_c.nam" },
    NamCapture { tone: "v10_t7_s",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t7_s.nam" },
    NamCapture { tone: "v10_t7_xs",       model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v10_t7_xs.nam" },
    NamCapture { tone: "v3_t10_main_c",   model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t10_main_c.nam" },
    NamCapture { tone: "v3_t10_main_s",   model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t10_main_s.nam" },
    NamCapture { tone: "v3_t10_main_xs",  model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t10_main_xs.nam" },
    NamCapture { tone: "v3_t3_c",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t3_c.nam" },
    NamCapture { tone: "v3_t3_s",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t3_s.nam" },
    NamCapture { tone: "v3_t3_xs",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t3_xs.nam" },
    NamCapture { tone: "v3_t5_c",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t5_c.nam" },
    NamCapture { tone: "v3_t5_s",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t5_s.nam" },
    NamCapture { tone: "v3_t5_xs",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t5_xs.nam" },
    NamCapture { tone: "v3_t7_c",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t7_c.nam" },
    NamCapture { tone: "v3_t7_s",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t7_s.nam" },
    NamCapture { tone: "v3_t7_xs",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v3_t7_xs.nam" },
    NamCapture { tone: "v5_t10_main_c",   model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t10_main_c.nam" },
    NamCapture { tone: "v5_t10_main_s",   model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t10_main_s.nam" },
    NamCapture { tone: "v5_t10_main_xs",  model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t10_main_xs.nam" },
    NamCapture { tone: "v5_t3_c",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t3_c.nam" },
    NamCapture { tone: "v5_t3_s",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t3_s.nam" },
    NamCapture { tone: "v5_t3_xs",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t3_xs.nam" },
    NamCapture { tone: "v5_t5_c",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t5_c.nam" },
    NamCapture { tone: "v5_t5_s",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t5_s.nam" },
    NamCapture { tone: "v5_t5_xs",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t5_xs.nam" },
    NamCapture { tone: "v5_t7_c",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t7_c.nam" },
    NamCapture { tone: "v5_t7_s",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t7_s.nam" },
    NamCapture { tone: "v5_t7_xs",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v5_t7_xs.nam" },
    NamCapture { tone: "v7_t0_c",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t0_c.nam" },
    NamCapture { tone: "v7_t0_s",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t0_s.nam" },
    NamCapture { tone: "v7_t0_xs",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t0_xs.nam" },
    NamCapture { tone: "v7_t10_main_c",   model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t10_main_c.nam" },
    NamCapture { tone: "v7_t10_main_s",   model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t10_main_s.nam" },
    NamCapture { tone: "v7_t10_main_xs",  model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t10_main_xs.nam" },
    NamCapture { tone: "v7_t3_c",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t3_c.nam" },
    NamCapture { tone: "v7_t3_s",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t3_s.nam" },
    NamCapture { tone: "v7_t3_xs",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t3_xs.nam" },
    NamCapture { tone: "v7_t5_c",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t5_c.nam" },
    NamCapture { tone: "v7_t5_s",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t5_s.nam" },
    NamCapture { tone: "v7_t5_xs",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t5_xs.nam" },
    NamCapture { tone: "v7_t7_c",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t7_c.nam" },
    NamCapture { tone: "v7_t7_s",         model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t7_s.nam" },
    NamCapture { tone: "v7_t7_xs",        model_path: "pedals/dallas_rangemaster/slammin_dallas_boost_v7_t7_xs.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("v10_t0_c"),
        &[
            ("v10_t0_c",        "V10 T0 C"),
            ("v10_t0_s",        "V10 T0 S"),
            ("v10_t0_xs",       "V10 T0 Xs"),
            ("v10_t10_main_c",  "V10 T10 Main C"),
            ("v10_t10_main_s",  "V10 T10 Main S"),
            ("v10_t10_main_xs", "V10 T10 Main Xs"),
            ("v10_t3_c",        "V10 T3 C"),
            ("v10_t3_s",        "V10 T3 S"),
            ("v10_t3_xs",       "V10 T3 Xs"),
            ("v10_t5_c",        "V10 T5 C"),
            ("v10_t5_s",        "V10 T5 S"),
            ("v10_t5_xs",       "V10 T5 Xs"),
            ("v10_t7_c",        "V10 T7 C"),
            ("v10_t7_s",        "V10 T7 S"),
            ("v10_t7_xs",       "V10 T7 Xs"),
            ("v3_t10_main_c",   "V3 T10 Main C"),
            ("v3_t10_main_s",   "V3 T10 Main S"),
            ("v3_t10_main_xs",  "V3 T10 Main Xs"),
            ("v3_t3_c",         "V3 T3 C"),
            ("v3_t3_s",         "V3 T3 S"),
            ("v3_t3_xs",        "V3 T3 Xs"),
            ("v3_t5_c",         "V3 T5 C"),
            ("v3_t5_s",         "V3 T5 S"),
            ("v3_t5_xs",        "V3 T5 Xs"),
            ("v3_t7_c",         "V3 T7 C"),
            ("v3_t7_s",         "V3 T7 S"),
            ("v3_t7_xs",        "V3 T7 Xs"),
            ("v5_t10_main_c",   "V5 T10 Main C"),
            ("v5_t10_main_s",   "V5 T10 Main S"),
            ("v5_t10_main_xs",  "V5 T10 Main Xs"),
            ("v5_t3_c",         "V5 T3 C"),
            ("v5_t3_s",         "V5 T3 S"),
            ("v5_t3_xs",        "V5 T3 Xs"),
            ("v5_t5_c",         "V5 T5 C"),
            ("v5_t5_s",         "V5 T5 S"),
            ("v5_t5_xs",        "V5 T5 Xs"),
            ("v5_t7_c",         "V5 T7 C"),
            ("v5_t7_s",         "V5 T7 S"),
            ("v5_t7_xs",        "V5 T7 Xs"),
            ("v7_t0_c",         "V7 T0 C"),
            ("v7_t0_s",         "V7 T0 S"),
            ("v7_t0_xs",        "V7 T0 Xs"),
            ("v7_t10_main_c",   "V7 T10 Main C"),
            ("v7_t10_main_s",   "V7 T10 Main S"),
            ("v7_t10_main_xs",  "V7 T10 Main Xs"),
            ("v7_t3_c",         "V7 T3 C"),
            ("v7_t3_s",         "V7 T3 S"),
            ("v7_t3_xs",        "V7 T3 Xs"),
            ("v7_t5_c",         "V7 T5 C"),
            ("v7_t5_s",         "V7 T5 S"),
            ("v7_t5_xs",        "V7 T5 Xs"),
            ("v7_t7_c",         "V7 T7 C"),
            ("v7_t7_s",         "V7 T7 S"),
            ("v7_t7_xs",        "V7 T7 Xs"),
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
