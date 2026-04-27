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

pub const MODEL_ID: &str = "nam_wampler_tumnus";
pub const DISPLAY_NAME: &str = "Wampler Tumnus";
const BRAND: &str = "wampler";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    bass: f32,
    gain: f32,
    mid: f32,
    treble: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Standard,
}

const BASS_MIN: f32 = 4.0;
const BASS_MAX: f32 = 5.0;
const GAIN_MIN: f32 = 2.0;
const GAIN_MAX: f32 = 10.0;
const MID_MIN: f32 = 5.0;
const MID_MAX: f32 = 6.0;
const TREBLE_MIN: f32 = 5.0;
const TREBLE_MAX: f32 = 7.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { bass: 4.0, gain: 10.0, mid: 6.0, treble: 7.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_4_m_6_t_7_l_6_g_10.nam" },
    GridCapture { bass: 4.0, gain: 8.0, mid: 6.0, treble: 7.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_4_m_6_t_7_l_6_g_8.nam" },
    GridCapture { bass: 5.0, gain: 10.0, mid: 5.0, treble: 5.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_5_m_5_t_5_l_6_g_10.nam" },
    GridCapture { bass: 5.0, gain: 8.0, mid: 5.0, treble: 5.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_5_m_5_t_5_l_6_g_8.nam" },
    GridCapture { bass: 5.0, gain: 10.0, mid: 6.0, treble: 6.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_5_m_6_t_6_l_6_g_10.nam" },
    GridCapture { bass: 5.0, gain: 8.0, mid: 6.0, treble: 6.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_5_m_6_t_6_l_6_g_8.nam" },
    GridCapture { bass: 4.0, gain: 10.0, mid: 6.0, treble: 7.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_10.nam" },
    GridCapture { bass: 4.0, gain: 2.0, mid: 6.0, treble: 7.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_2.nam" },
    GridCapture { bass: 4.0, gain: 3.0, mid: 6.0, treble: 7.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_3.nam" },
    GridCapture { bass: 4.0, gain: 4.0, mid: 6.0, treble: 7.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_4.nam" },
    GridCapture { bass: 4.0, gain: 5.0, mid: 6.0, treble: 7.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_5.nam" },
    GridCapture { bass: 4.0, gain: 6.0, mid: 6.0, treble: 7.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_6.nam" },
    GridCapture { bass: 4.0, gain: 7.0, mid: 6.0, treble: 7.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_7.nam" },
    GridCapture { bass: 4.0, gain: 8.0, mid: 6.0, treble: 7.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_8.nam" },
    GridCapture { bass: 5.0, gain: 10.0, mid: 5.0, treble: 5.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_10.nam" },
    GridCapture { bass: 5.0, gain: 2.0, mid: 5.0, treble: 5.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_2.nam" },
    GridCapture { bass: 5.0, gain: 3.0, mid: 5.0, treble: 5.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_3.nam" },
    GridCapture { bass: 5.0, gain: 4.0, mid: 5.0, treble: 5.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_4.nam" },
    GridCapture { bass: 5.0, gain: 5.0, mid: 5.0, treble: 5.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_5.nam" },
    GridCapture { bass: 5.0, gain: 6.0, mid: 5.0, treble: 5.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_6.nam" },
    GridCapture { bass: 5.0, gain: 7.0, mid: 5.0, treble: 5.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_7.nam" },
    GridCapture { bass: 5.0, gain: 8.0, mid: 5.0, treble: 5.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_8.nam" },
    GridCapture { bass: 5.0, gain: 10.0, mid: 6.0, treble: 6.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_10.nam" },
    GridCapture { bass: 5.0, gain: 2.0, mid: 6.0, treble: 6.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_2.nam" },
    GridCapture { bass: 5.0, gain: 3.0, mid: 6.0, treble: 6.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_3.nam" },
    GridCapture { bass: 5.0, gain: 4.0, mid: 6.0, treble: 6.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_4.nam" },
    GridCapture { bass: 5.0, gain: 5.0, mid: 6.0, treble: 6.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_5.nam" },
    GridCapture { bass: 5.0, gain: 6.0, mid: 6.0, treble: 6.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_6.nam" },
    GridCapture { bass: 5.0, gain: 7.0, mid: 6.0, treble: 6.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_7.nam" },
    GridCapture { bass: 5.0, gain: 8.0, mid: 6.0, treble: 6.0, size: NamSize::Standard, model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_8.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("bass", "Bass", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("gain", "Gain", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("mid", "Mid", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("treble", "Treble", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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
    let bass_pct = required_f32(params, "bass").map_err(anyhow::Error::msg)?;
    let gain_pct = required_f32(params, "gain").map_err(anyhow::Error::msg)?;
    let mid_pct = required_f32(params, "mid").map_err(anyhow::Error::msg)?;
    let treble_pct = required_f32(params, "treble").map_err(anyhow::Error::msg)?;
    let bass = BASS_MIN + (bass_pct / 100.0) * (BASS_MAX - BASS_MIN);
    let gain = GAIN_MIN + (gain_pct / 100.0) * (GAIN_MAX - GAIN_MIN);
    let mid = MID_MIN + (mid_pct / 100.0) * (MID_MAX - MID_MIN);
    let treble = TREBLE_MIN + (treble_pct / 100.0) * (TREBLE_MAX - TREBLE_MIN);
    let _size = NamSize::Standard;
    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::Standard);
    candidates
        .min_by(|a, b| {
            let da = (a.bass - bass).powi(2) + (a.gain - gain).powi(2) + (a.mid - mid).powi(2) + (a.treble - treble).powi(2);
            let db = (b.bass - bass).powi(2) + (b.gain - gain).powi(2) + (b.mid - mid).powi(2) + (b.treble - treble).powi(2);
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

