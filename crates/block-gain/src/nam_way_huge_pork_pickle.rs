use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_way_huge_pork_pickle";
pub const DISPLAY_NAME: &str = "Way Huge Pork Pickle";
const BRAND: &str = "way_huge";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "pickle_high_drive_no_blend",   model_path: "pedals/way_huge_pork_pickle/pickle_high_drive_no_blend.nam" },
    NamCapture { tone: "pickle_high_drive_with_blend", model_path: "pedals/way_huge_pork_pickle/pickle_high_drive_with_blend.nam" },
    NamCapture { tone: "pickle_low_drive_no_blend",    model_path: "pedals/way_huge_pork_pickle/pickle_low_drive_no_blend.nam" },
    NamCapture { tone: "pickle_low_drive_with_blend",  model_path: "pedals/way_huge_pork_pickle/pickle_low_drive_with_blend.nam" },
    NamCapture { tone: "pickle_mid_drive_no_blend",    model_path: "pedals/way_huge_pork_pickle/pickle_mid_drive_no_blend.nam" },
    NamCapture { tone: "pickle_mid_drive_with_blend",  model_path: "pedals/way_huge_pork_pickle/pickle_mid_drive_with_blend.nam" },
    NamCapture { tone: "pork_high_drive_no_blend",     model_path: "pedals/way_huge_pork_pickle/pork_high_drive_no_blend.nam" },
    NamCapture { tone: "pork_high_drive_with_blend",   model_path: "pedals/way_huge_pork_pickle/pork_high_drive_with_blend.nam" },
    NamCapture { tone: "pork_low_drive_no_blend",      model_path: "pedals/way_huge_pork_pickle/pork_low_drive_no_blend.nam" },
    NamCapture { tone: "pork_low_drive_with_blend",    model_path: "pedals/way_huge_pork_pickle/pork_low_drive_with_blend.nam" },
    NamCapture { tone: "pork_mid_drive_no_blend",      model_path: "pedals/way_huge_pork_pickle/pork_mid_drive_no_blend.nam" },
    NamCapture { tone: "pork_mid_drive_with_blend",    model_path: "pedals/way_huge_pork_pickle/pork_mid_drive_with_blend.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("pickle_high_drive_no_blend"),
        &[
            ("pickle_high_drive_no_blend",   "Pickle High Drive No Blend"),
            ("pickle_high_drive_with_blend", "Pickle High Drive With Blend"),
            ("pickle_low_drive_no_blend",    "Pickle Low Drive No Blend"),
            ("pickle_low_drive_with_blend",  "Pickle Low Drive With Blend"),
            ("pickle_mid_drive_no_blend",    "Pickle Mid Drive No Blend"),
            ("pickle_mid_drive_with_blend",  "Pickle Mid Drive With Blend"),
            ("pork_high_drive_no_blend",     "Pork High Drive No Blend"),
            ("pork_high_drive_with_blend",   "Pork High Drive With Blend"),
            ("pork_low_drive_no_blend",      "Pork Low Drive No Blend"),
            ("pork_low_drive_with_blend",    "Pork Low Drive With Blend"),
            ("pork_mid_drive_no_blend",      "Pork Mid Drive No Blend"),
            ("pork_mid_drive_with_blend",    "Pork Mid Drive With Blend"),
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
