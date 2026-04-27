use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_analog_man_sun_face";
pub const DISPLAY_NAME: &str = "Analog Man Sun Face";
const BRAND: &str = "analogman";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "v7_f10_c10",        model_path: "pedals/analog_man_sun_face/sunface_bc183_v7_f10_c10.nam" },
    NamCapture { tone: "v7_f10_c3",         model_path: "pedals/analog_man_sun_face/sunface_bc183_v7_f10_c3.nam" },
    NamCapture { tone: "v7_f10_c5",         model_path: "pedals/analog_man_sun_face/sunface_bc183_v7_f10_c5.nam" },
    NamCapture { tone: "v7_f10_c8",         model_path: "pedals/analog_man_sun_face/sunface_bc183_v7_f10_c8.nam" },
    NamCapture { tone: "v8_f9_c10_cleanup", model_path: "pedals/analog_man_sun_face/sunface_bc183_v8_f9_c10_cleanup.nam" },
    NamCapture { tone: "v8_f9_c6_cleanup",  model_path: "pedals/analog_man_sun_face/sunface_bc183_v8_f9_c6_cleanup.nam" },
    NamCapture { tone: "v8_f9_c9_cleanup",  model_path: "pedals/analog_man_sun_face/sunface_bc183_v8_f9_c9_cleanup.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("v7_f10_c10"),
        &[
            ("v7_f10_c10",        "V7 F10 C10"),
            ("v7_f10_c3",         "V7 F10 C3"),
            ("v7_f10_c5",         "V7 F10 C5"),
            ("v7_f10_c8",         "V7 F10 C8"),
            ("v8_f9_c10_cleanup", "V8 F9 C10 Cleanup"),
            ("v8_f9_c6_cleanup",  "V8 F9 C6 Cleanup"),
            ("v8_f9_c9_cleanup",  "V8 F9 C9 Cleanup"),
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
