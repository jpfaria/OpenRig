use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_wampler_tumnus";
pub const DISPLAY_NAME: &str = "Wampler Tumnus";
const BRAND: &str = "wampler";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "hot_b_4_m_6_t_7_l_6_g_10", model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_4_m_6_t_7_l_6_g_10.nam" },
    NamCapture { tone: "hot_b_4_m_6_t_7_l_6_g_8",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_4_m_6_t_7_l_6_g_8.nam" },
    NamCapture { tone: "hot_b_5_m_5_t_5_l_6_g_10", model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_5_m_5_t_5_l_6_g_10.nam" },
    NamCapture { tone: "hot_b_5_m_5_t_5_l_6_g_8",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_5_m_5_t_5_l_6_g_8.nam" },
    NamCapture { tone: "hot_b_5_m_6_t_6_l_6_g_10", model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_5_m_6_t_6_l_6_g_10.nam" },
    NamCapture { tone: "hot_b_5_m_6_t_6_l_6_g_8",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_hot_b_5_m_6_t_6_l_6_g_8.nam" },
    NamCapture { tone: "nrm_b_4_m_6_t_7_l_6_g_10", model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_10.nam" },
    NamCapture { tone: "nrm_b_4_m_6_t_7_l_6_g_2",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_2.nam" },
    NamCapture { tone: "nrm_b_4_m_6_t_7_l_6_g_3",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_3.nam" },
    NamCapture { tone: "nrm_b_4_m_6_t_7_l_6_g_4",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_4.nam" },
    NamCapture { tone: "nrm_b_4_m_6_t_7_l_6_g_5",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_5.nam" },
    NamCapture { tone: "nrm_b_4_m_6_t_7_l_6_g_6",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_6.nam" },
    NamCapture { tone: "nrm_b_4_m_6_t_7_l_6_g_7",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_7.nam" },
    NamCapture { tone: "nrm_b_4_m_6_t_7_l_6_g_8",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_4_m_6_t_7_l_6_g_8.nam" },
    NamCapture { tone: "nrm_b_5_m_5_t_5_l_6_g_10", model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_10.nam" },
    NamCapture { tone: "nrm_b_5_m_5_t_5_l_6_g_2",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_2.nam" },
    NamCapture { tone: "nrm_b_5_m_5_t_5_l_6_g_3",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_3.nam" },
    NamCapture { tone: "nrm_b_5_m_5_t_5_l_6_g_4",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_4.nam" },
    NamCapture { tone: "nrm_b_5_m_5_t_5_l_6_g_5",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_5.nam" },
    NamCapture { tone: "nrm_b_5_m_5_t_5_l_6_g_6",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_6.nam" },
    NamCapture { tone: "nrm_b_5_m_5_t_5_l_6_g_7",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_7.nam" },
    NamCapture { tone: "nrm_b_5_m_5_t_5_l_6_g_8",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_5_t_5_l_6_g_8.nam" },
    NamCapture { tone: "nrm_b_5_m_6_t_6_l_6_g_10", model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_10.nam" },
    NamCapture { tone: "nrm_b_5_m_6_t_6_l_6_g_2",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_2.nam" },
    NamCapture { tone: "nrm_b_5_m_6_t_6_l_6_g_3",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_3.nam" },
    NamCapture { tone: "nrm_b_5_m_6_t_6_l_6_g_4",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_4.nam" },
    NamCapture { tone: "nrm_b_5_m_6_t_6_l_6_g_5",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_5.nam" },
    NamCapture { tone: "nrm_b_5_m_6_t_6_l_6_g_6",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_6.nam" },
    NamCapture { tone: "nrm_b_5_m_6_t_6_l_6_g_7",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_7.nam" },
    NamCapture { tone: "nrm_b_5_m_6_t_6_l_6_g_8",  model_path: "pedals/wampler_tumnus/tumnus_deluxe_nrm_b_5_m_6_t_6_l_6_g_8.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("hot_b_4_m_6_t_7_l_6_g_10"),
        &[
            ("hot_b_4_m_6_t_7_l_6_g_10", "Hot B 4 M 6 T 7 L 6 G 10"),
            ("hot_b_4_m_6_t_7_l_6_g_8",  "Hot B 4 M 6 T 7 L 6 G 8"),
            ("hot_b_5_m_5_t_5_l_6_g_10", "Hot B 5 M 5 T 5 L 6 G 10"),
            ("hot_b_5_m_5_t_5_l_6_g_8",  "Hot B 5 M 5 T 5 L 6 G 8"),
            ("hot_b_5_m_6_t_6_l_6_g_10", "Hot B 5 M 6 T 6 L 6 G 10"),
            ("hot_b_5_m_6_t_6_l_6_g_8",  "Hot B 5 M 6 T 6 L 6 G 8"),
            ("nrm_b_4_m_6_t_7_l_6_g_10", "Nrm B 4 M 6 T 7 L 6 G 10"),
            ("nrm_b_4_m_6_t_7_l_6_g_2",  "Nrm B 4 M 6 T 7 L 6 G 2"),
            ("nrm_b_4_m_6_t_7_l_6_g_3",  "Nrm B 4 M 6 T 7 L 6 G 3"),
            ("nrm_b_4_m_6_t_7_l_6_g_4",  "Nrm B 4 M 6 T 7 L 6 G 4"),
            ("nrm_b_4_m_6_t_7_l_6_g_5",  "Nrm B 4 M 6 T 7 L 6 G 5"),
            ("nrm_b_4_m_6_t_7_l_6_g_6",  "Nrm B 4 M 6 T 7 L 6 G 6"),
            ("nrm_b_4_m_6_t_7_l_6_g_7",  "Nrm B 4 M 6 T 7 L 6 G 7"),
            ("nrm_b_4_m_6_t_7_l_6_g_8",  "Nrm B 4 M 6 T 7 L 6 G 8"),
            ("nrm_b_5_m_5_t_5_l_6_g_10", "Nrm B 5 M 5 T 5 L 6 G 10"),
            ("nrm_b_5_m_5_t_5_l_6_g_2",  "Nrm B 5 M 5 T 5 L 6 G 2"),
            ("nrm_b_5_m_5_t_5_l_6_g_3",  "Nrm B 5 M 5 T 5 L 6 G 3"),
            ("nrm_b_5_m_5_t_5_l_6_g_4",  "Nrm B 5 M 5 T 5 L 6 G 4"),
            ("nrm_b_5_m_5_t_5_l_6_g_5",  "Nrm B 5 M 5 T 5 L 6 G 5"),
            ("nrm_b_5_m_5_t_5_l_6_g_6",  "Nrm B 5 M 5 T 5 L 6 G 6"),
            ("nrm_b_5_m_5_t_5_l_6_g_7",  "Nrm B 5 M 5 T 5 L 6 G 7"),
            ("nrm_b_5_m_5_t_5_l_6_g_8",  "Nrm B 5 M 5 T 5 L 6 G 8"),
            ("nrm_b_5_m_6_t_6_l_6_g_10", "Nrm B 5 M 6 T 6 L 6 G 10"),
            ("nrm_b_5_m_6_t_6_l_6_g_2",  "Nrm B 5 M 6 T 6 L 6 G 2"),
            ("nrm_b_5_m_6_t_6_l_6_g_3",  "Nrm B 5 M 6 T 6 L 6 G 3"),
            ("nrm_b_5_m_6_t_6_l_6_g_4",  "Nrm B 5 M 6 T 6 L 6 G 4"),
            ("nrm_b_5_m_6_t_6_l_6_g_5",  "Nrm B 5 M 6 T 6 L 6 G 5"),
            ("nrm_b_5_m_6_t_6_l_6_g_6",  "Nrm B 5 M 6 T 6 L 6 G 6"),
            ("nrm_b_5_m_6_t_6_l_6_g_7",  "Nrm B 5 M 6 T 6 L 6 G 7"),
            ("nrm_b_5_m_6_t_6_l_6_g_8",  "Nrm B 5 M 6 T 6 L 6 G 8"),
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
