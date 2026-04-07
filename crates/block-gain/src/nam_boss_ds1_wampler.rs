use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "boss_ds1_wampler";
pub const DISPLAY_NAME: &str = "DS-1 Wampler JCM Mod";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ds1WamplerParams {
    pub tone: &'static str,
    pub dist: &'static str,
}

struct Ds1WamplerCapture {
    params: Ds1WamplerParams,
    model_path: &'static str,
}

const CAPTURES: &[Ds1WamplerCapture] = &[
    Ds1WamplerCapture { params: Ds1WamplerParams { tone: "2", dist: "0"  }, model_path: "pedals/boss_ds1_wampler/ds1w_t2_d0.nam" },
    Ds1WamplerCapture { params: Ds1WamplerParams { tone: "2", dist: "5"  }, model_path: "pedals/boss_ds1_wampler/ds1w_t2_d5.nam" },
    Ds1WamplerCapture { params: Ds1WamplerParams { tone: "2", dist: "10" }, model_path: "pedals/boss_ds1_wampler/ds1w_t2_d10.nam" },
    Ds1WamplerCapture { params: Ds1WamplerParams { tone: "6", dist: "0"  }, model_path: "pedals/boss_ds1_wampler/ds1w_t6_d0.nam" },
    Ds1WamplerCapture { params: Ds1WamplerParams { tone: "6", dist: "5"  }, model_path: "pedals/boss_ds1_wampler/ds1w_t6_d5.nam" },
    Ds1WamplerCapture { params: Ds1WamplerParams { tone: "6", dist: "10" }, model_path: "pedals/boss_ds1_wampler/ds1w_t6_d10.nam" },
    Ds1WamplerCapture { params: Ds1WamplerParams { tone: "8", dist: "0"  }, model_path: "pedals/boss_ds1_wampler/ds1w_t8_d0.nam" },
    Ds1WamplerCapture { params: Ds1WamplerParams { tone: "8", dist: "5"  }, model_path: "pedals/boss_ds1_wampler/ds1w_t8_d5.nam" },
    Ds1WamplerCapture { params: Ds1WamplerParams { tone: "8", dist: "10" }, model_path: "pedals/boss_ds1_wampler/ds1w_t8_d10.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "tone",
            "Tone",
            Some("Pedal"),
            Some("6"),
            &[("2", "Dark"), ("6", "Neutral"), ("8", "Bright")],
        ),
        enum_parameter(
            "dist",
            "Dist",
            Some("Pedal"),
            Some("5"),
            &[("0", "Clean"), ("5", "Medium"), ("10", "High")],
        ),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static Ds1WamplerCapture> {
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    let dist = required_string(params, "dist").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.params.tone == tone && c.params.dist == dist)
        .ok_or_else(|| {
            anyhow!(
                "gain model '{}' does not support tone='{}' dist='{}'",
                MODEL_ID, tone, dist
            )
        })
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
