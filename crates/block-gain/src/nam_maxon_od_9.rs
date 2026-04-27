use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_maxon_od_9";
pub const DISPLAY_NAME: &str = "Maxon OD-9";
const BRAND: &str = "maxon";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "d0_t5_l10",  model_path: "pedals/maxon_od_9/maxon_od_9_vintage_rc4558p_a_a_d0_t5_l10.nam" },
    NamCapture { tone: "d0_t7_l10",  model_path: "pedals/maxon_od_9/maxon_od_9_vintage_rc4558p_a_a_d0_t7_l10.nam" },
    NamCapture { tone: "d10_t5_l10", model_path: "pedals/maxon_od_9/maxon_od_9_vintage_rc4558p_a_a_d10_t5_l10.nam" },
    NamCapture { tone: "d10_t7_l10", model_path: "pedals/maxon_od_9/maxon_od_9_vintage_rc4558p_a_a_d10_t7_l10.nam" },
    NamCapture { tone: "d2_t5_l10",  model_path: "pedals/maxon_od_9/maxon_od_9_vintage_rc4558p_a_a_d2_t5_l10.nam" },
    NamCapture { tone: "d2_t7_l10",  model_path: "pedals/maxon_od_9/maxon_od_9_vintage_rc4558p_a_a_d2_t7_l10.nam" },
    NamCapture { tone: "d5_t5_l10",  model_path: "pedals/maxon_od_9/maxon_od_9_vintage_rc4558p_a_a_d5_t5_l10.nam" },
    NamCapture { tone: "d5_t7_l10",  model_path: "pedals/maxon_od_9/maxon_od_9_vintage_rc4558p_a_a_d5_t7_l10.nam" },
    NamCapture { tone: "d8_t5_l10",  model_path: "pedals/maxon_od_9/maxon_od_9_vintage_rc4558p_a_a_d8_t5_l10.nam" },
    NamCapture { tone: "d8_t7_l10",  model_path: "pedals/maxon_od_9/maxon_od_9_vintage_rc4558p_a_a_d8_t7_l10.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("d0_t5_l10"),
        &[
            ("d0_t5_l10",  "D0 T5 L10"),
            ("d0_t7_l10",  "D0 T7 L10"),
            ("d10_t5_l10", "D10 T5 L10"),
            ("d10_t7_l10", "D10 T7 L10"),
            ("d2_t5_l10",  "D2 T5 L10"),
            ("d2_t7_l10",  "D2 T7 L10"),
            ("d5_t5_l10",  "D5 T5 L10"),
            ("d5_t7_l10",  "D5 T7 L10"),
            ("d8_t5_l10",  "D8 T5 L10"),
            ("d8_t7_l10",  "D8 T7 L10"),
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
