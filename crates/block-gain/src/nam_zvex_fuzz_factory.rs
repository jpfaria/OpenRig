use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_zvex_fuzz_factory";
pub const DISPLAY_NAME: &str = "ZVEX Fuzz Factory";
const BRAND: &str = "zvex";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "12_00_c_3_00_f_12_00",  model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_12_00_c_3_00_f_12_00_.nam" },
    NamCapture { tone: "12_00_c_3_00_f_min_s",  model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_12_00_c_3_00_f_min_s_.nam" },
    NamCapture { tone: "12_30_c_min_f_min_s_m", model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_12_30_c_min_f_min_s_m.nam" },
    NamCapture { tone: "3_00_c_9_00_f_3_00_s",  model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_3_00_c_9_00_f_3_00_s_.nam" },
    NamCapture { tone: "9_00_c_4_00_f_9_00_s",  model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_9_00_c_4_00_f_9_00_s_.nam" },
    NamCapture { tone: "min_c_4_00_f_max_s_2",  model_path: "pedals/zvex_fuzz_factory/fuzz_factory_clone_v_10_00_g_min_c_4_00_f_max_s_2_.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("12_00_c_3_00_f_12_00"),
        &[
            ("12_00_c_3_00_f_12_00",  "12 00 C 3 00 F 12 00 "),
            ("12_00_c_3_00_f_min_s",  "12 00 C 3 00 F Min S "),
            ("12_30_c_min_f_min_s_m", "12 30 C Min F Min S M"),
            ("3_00_c_9_00_f_3_00_s",  "3 00 C 9 00 F 3 00 S "),
            ("9_00_c_4_00_f_9_00_s",  "9 00 C 4 00 F 9 00 S "),
            ("min_c_4_00_f_max_s_2",  "Min C 4 00 F Max S 2 "),
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
