use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string, 
    ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "nam_big_muff";
pub const DISPLAY_NAME: &str = "Big Muff";
const BRAND: &str = "ehx";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    sustain: f32,
    tone: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Feather,
    Standard,
}

const SUSTAIN_MIN: f32 = 0.0;
const SUSTAIN_MAX: f32 = 10.0;
const TONE_MIN: f32 = 2.0;
const TONE_MAX: f32 = 7.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { sustain: 0.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_0.nam" },
    GridCapture { sustain: 0.0, tone: 2.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_0_feather.nam" },
    GridCapture { sustain: 10.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_10.nam" },
    GridCapture { sustain: 10.0, tone: 2.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_10_feather.nam" },
    GridCapture { sustain: 2.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_2.nam" },
    GridCapture { sustain: 2.0, tone: 2.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_2_feather.nam" },
    GridCapture { sustain: 5.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_5.nam" },
    GridCapture { sustain: 5.0, tone: 2.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_5_feather.nam" },
    GridCapture { sustain: 8.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_8.nam" },
    GridCapture { sustain: 8.0, tone: 2.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_2_s_8_feather.nam" },
    GridCapture { sustain: 0.0, tone: 3.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_0.nam" },
    GridCapture { sustain: 0.0, tone: 3.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_0_feather.nam" },
    GridCapture { sustain: 10.0, tone: 3.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_10.nam" },
    GridCapture { sustain: 10.0, tone: 3.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_10_feather.nam" },
    GridCapture { sustain: 2.0, tone: 3.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_2.nam" },
    GridCapture { sustain: 2.0, tone: 3.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_2_feather.nam" },
    GridCapture { sustain: 5.0, tone: 3.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_5.nam" },
    GridCapture { sustain: 5.0, tone: 3.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_5_feather.nam" },
    GridCapture { sustain: 8.0, tone: 3.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_8.nam" },
    GridCapture { sustain: 8.0, tone: 3.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_3_s_8_feather.nam" },
    GridCapture { sustain: 0.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_0.nam" },
    GridCapture { sustain: 0.0, tone: 4.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_0_feather.nam" },
    GridCapture { sustain: 10.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_10.nam" },
    GridCapture { sustain: 10.0, tone: 4.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_10_feather.nam" },
    GridCapture { sustain: 2.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_2.nam" },
    GridCapture { sustain: 2.0, tone: 4.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_2_feather.nam" },
    GridCapture { sustain: 5.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_5.nam" },
    GridCapture { sustain: 5.0, tone: 4.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_5_feather.nam" },
    GridCapture { sustain: 8.0, tone: 4.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_8.nam" },
    GridCapture { sustain: 8.0, tone: 4.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_4_s_8_feather.nam" },
    GridCapture { sustain: 0.0, tone: 5.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_0.nam" },
    GridCapture { sustain: 0.0, tone: 5.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_0_feather.nam" },
    GridCapture { sustain: 10.0, tone: 5.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_10.nam" },
    GridCapture { sustain: 10.0, tone: 5.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_10_feather.nam" },
    GridCapture { sustain: 2.0, tone: 5.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_2.nam" },
    GridCapture { sustain: 2.0, tone: 5.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_2_feather.nam" },
    GridCapture { sustain: 5.0, tone: 5.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_5.nam" },
    GridCapture { sustain: 5.0, tone: 5.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_5_feather.nam" },
    GridCapture { sustain: 8.0, tone: 5.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_8.nam" },
    GridCapture { sustain: 8.0, tone: 5.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_5_s_8_feather.nam" },
    GridCapture { sustain: 0.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_0.nam" },
    GridCapture { sustain: 0.0, tone: 6.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_0_feather.nam" },
    GridCapture { sustain: 10.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_10.nam" },
    GridCapture { sustain: 10.0, tone: 6.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_10_feather.nam" },
    GridCapture { sustain: 2.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_2.nam" },
    GridCapture { sustain: 2.0, tone: 6.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_2_feather.nam" },
    GridCapture { sustain: 5.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_5.nam" },
    GridCapture { sustain: 5.0, tone: 6.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_5_feather.nam" },
    GridCapture { sustain: 8.0, tone: 6.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_8.nam" },
    GridCapture { sustain: 8.0, tone: 6.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_6_s_8_feather.nam" },
    GridCapture { sustain: 0.0, tone: 7.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_0.nam" },
    GridCapture { sustain: 0.0, tone: 7.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_0_feather.nam" },
    GridCapture { sustain: 10.0, tone: 7.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_10.nam" },
    GridCapture { sustain: 10.0, tone: 7.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_10_feather.nam" },
    GridCapture { sustain: 2.0, tone: 7.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_2.nam" },
    GridCapture { sustain: 2.0, tone: 7.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_2_feather.nam" },
    GridCapture { sustain: 5.0, tone: 7.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_5.nam" },
    GridCapture { sustain: 5.0, tone: 7.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_5_feather.nam" },
    GridCapture { sustain: 8.0, tone: 7.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_8.nam" },
    GridCapture { sustain: 8.0, tone: 7.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_7_s_8_feather.nam" },
    GridCapture { sustain: 0.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_0.nam" },
    GridCapture { sustain: 0.0, tone: 2.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_0_feather.nam" },
    GridCapture { sustain: 10.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_10.nam" },
    GridCapture { sustain: 10.0, tone: 2.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_10_feather.nam" },
    GridCapture { sustain: 2.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_2.nam" },
    GridCapture { sustain: 2.0, tone: 2.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_2_feather.nam" },
    GridCapture { sustain: 5.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_5.nam" },
    GridCapture { sustain: 5.0, tone: 2.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_5_feather.nam" },
    GridCapture { sustain: 8.0, tone: 2.0, size: NamSize::Standard, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_8.nam" },
    GridCapture { sustain: 8.0, tone: 2.0, size: NamSize::Feather, model_path: "pedals/big_muff/ehx_ic_big_muff_v_6_t_byp_s_8_feather.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("sustain", "Sustain", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("tone", "Tone", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        enum_parameter("size", "Model Size", Some("Capture"), Some("standard"), &[("feather", "Feather"), ("standard", "Standard")]),
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
    let sustain_pct = required_f32(params, "sustain").map_err(anyhow::Error::msg)?;
    let tone_pct = required_f32(params, "tone").map_err(anyhow::Error::msg)?;
    let sustain = SUSTAIN_MIN + (sustain_pct / 100.0) * (SUSTAIN_MAX - SUSTAIN_MIN);
    let tone = TONE_MIN + (tone_pct / 100.0) * (TONE_MAX - TONE_MIN);
    let size_str = required_string(params, "size").map_err(anyhow::Error::msg)?;
    let size = match size_str.as_str() {
        "feather" => NamSize::Feather,
        "standard" => NamSize::Standard,
        other => return Err(anyhow!("unknown size '{}'", other)),
    };
    let candidates = CAPTURES.iter().filter(|c| c.size == size);
    candidates
        .min_by(|a, b| {
            let da = (a.sustain - sustain).powi(2) + (a.tone - tone).powi(2);
            let db = (b.sustain - sustain).powi(2) + (b.tone - tone).powi(2);
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

