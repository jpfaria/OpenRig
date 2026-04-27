use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_benson_preamp";
pub const DISPLAY_NAME: &str = "Benson Preamp";
const BRAND: &str = "benson";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "01_t6b6d3v6", model_path: "pedals/benson_preamp/benson_preamp_01_t6b6d3v6.nam" },
    NamCapture { tone: "02_t6b6d6v6", model_path: "pedals/benson_preamp/benson_preamp_02_t6b6d6v6.nam" },
    NamCapture { tone: "03_t6b6d9v6", model_path: "pedals/benson_preamp/benson_preamp_03_t6b6d9v6.nam" },
    NamCapture { tone: "04_t9b3d3v6", model_path: "pedals/benson_preamp/benson_preamp_04_t9b3d3v6.nam" },
    NamCapture { tone: "05_t9b3d6v6", model_path: "pedals/benson_preamp/benson_preamp_05_t9b3d6v6.nam" },
    NamCapture { tone: "06_t9b3d9v6", model_path: "pedals/benson_preamp/benson_preamp_06_t9b3d9v6.nam" },
    NamCapture { tone: "07_t4b8d3v6", model_path: "pedals/benson_preamp/benson_preamp_07_t4b8d3v6.nam" },
    NamCapture { tone: "08_t4b8d6v6", model_path: "pedals/benson_preamp/benson_preamp_08_t4b8d6v6.nam" },
    NamCapture { tone: "09_t4b8d9v6", model_path: "pedals/benson_preamp/benson_preamp_09_t4b8d9v6.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("01_t6b6d3v6"),
        &[
            ("01_t6b6d3v6", "01 T6B6D3V6"),
            ("02_t6b6d6v6", "02 T6B6D6V6"),
            ("03_t6b6d9v6", "03 T6B6D9V6"),
            ("04_t9b3d3v6", "04 T9B3D3V6"),
            ("05_t9b3d6v6", "05 T9B3D6V6"),
            ("06_t9b3d9v6", "06 T9B3D9V6"),
            ("07_t4b8d3v6", "07 T4B8D3V6"),
            ("08_t4b8d6v6", "08 T4B8D6V6"),
            ("09_t4b8d9v6", "09 T4B8D9V6"),
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
