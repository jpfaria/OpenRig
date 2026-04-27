use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_browne_protein";
pub const DISPLAY_NAME: &str = "Browne Protein";
const BRAND: &str = "browne";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "blue_gain_1", model_path: "pedals/browne_protein/blue_gain_1.nam" },
    NamCapture { tone: "blue_gain_2", model_path: "pedals/browne_protein/blue_gain_2.nam" },
    NamCapture { tone: "blue_gain_3", model_path: "pedals/browne_protein/blue_gain_3.nam" },
    NamCapture { tone: "blue_gain_4", model_path: "pedals/browne_protein/blue_gain_4.nam" },
    NamCapture { tone: "blue_gain_5", model_path: "pedals/browne_protein/blue_gain_5.nam" },
    NamCapture { tone: "blue_gain_6", model_path: "pedals/browne_protein/blue_gain_6.nam" },
    NamCapture { tone: "green_gain_1", model_path: "pedals/browne_protein/green_gain_1.nam" },
    NamCapture { tone: "green_gain_2", model_path: "pedals/browne_protein/green_gain_2.nam" },
    NamCapture { tone: "green_gain_3", model_path: "pedals/browne_protein/green_gain_3.nam" },
    NamCapture { tone: "green_gain_4", model_path: "pedals/browne_protein/green_gain_4.nam" },
    NamCapture { tone: "green_gain_5", model_path: "pedals/browne_protein/green_gain_5.nam" },
    NamCapture { tone: "green_gain_6", model_path: "pedals/browne_protein/green_gain_6.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("blue_gain_1"),
        &[
            ("blue_gain_1", "Blue Gain 1"),
            ("blue_gain_2", "Blue Gain 2"),
            ("blue_gain_3", "Blue Gain 3"),
            ("blue_gain_4", "Blue Gain 4"),
            ("blue_gain_5", "Blue Gain 5"),
            ("blue_gain_6", "Blue Gain 6"),
            ("green_gain_1", "Green Gain 1"),
            ("green_gain_2", "Green Gain 2"),
            ("green_gain_3", "Green Gain 3"),
            ("green_gain_4", "Green Gain 4"),
            ("green_gain_5", "Green Gain 5"),
            ("green_gain_6", "Green Gain 6"),
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
