use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{bool_parameter, enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "jhs_bonsai";
pub const DISPLAY_NAME: &str = "Bonsai (9 TS)";
const BRAND: &str = "jhs";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BonsaiParams {
    pub mode: &'static str,
    pub boost: bool,
}

struct BonsaiCapture {
    params: BonsaiParams,
    model_path: &'static str,
}

const CAPTURES: &[BonsaiCapture] = &[
    BonsaiCapture { params: BonsaiParams { mode: "808",   boost: false }, model_path: "pedals/jhs_bonsai/bonsai_808_neutral.nam" },
    BonsaiCapture { params: BonsaiParams { mode: "808",   boost: true  }, model_path: "pedals/jhs_bonsai/bonsai_808_boost.nam" },
    BonsaiCapture { params: BonsaiParams { mode: "ts9",   boost: false }, model_path: "pedals/jhs_bonsai/bonsai_ts9_neutral.nam" },
    BonsaiCapture { params: BonsaiParams { mode: "ts9",   boost: true  }, model_path: "pedals/jhs_bonsai/bonsai_ts9_boost.nam" },
    BonsaiCapture { params: BonsaiParams { mode: "od1",   boost: false }, model_path: "pedals/jhs_bonsai/bonsai_od1_neutral.nam" },
    BonsaiCapture { params: BonsaiParams { mode: "od1",   boost: true  }, model_path: "pedals/jhs_bonsai/bonsai_od1_boost.nam" },
    BonsaiCapture { params: BonsaiParams { mode: "jhs",   boost: false }, model_path: "pedals/jhs_bonsai/bonsai_jhs_neutral.nam" },
    BonsaiCapture { params: BonsaiParams { mode: "jhs",   boost: true  }, model_path: "pedals/jhs_bonsai/bonsai_jhs_boost.nam" },
    BonsaiCapture { params: BonsaiParams { mode: "keeley", boost: false }, model_path: "pedals/jhs_bonsai/bonsai_keeley_neutral.nam" },
    BonsaiCapture { params: BonsaiParams { mode: "keeley", boost: true  }, model_path: "pedals/jhs_bonsai/bonsai_keeley_boost.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "mode",
            "Mode",
            Some("Pedal"),
            Some("808"),
            &[
                ("808",    "TS808"),
                ("ts9",    "TS9"),
                ("od1",    "OD-1"),
                ("jhs",    "JHS"),
                ("keeley", "Keeley"),
            ],
        ),
        bool_parameter("boost", "Boost", Some("Pedal"), Some(false)),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static BonsaiCapture> {
    let mode = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    let boost = params.get_bool("boost").unwrap_or(false);
    CAPTURES
        .iter()
        .find(|c| c.params.mode == mode && c.params.boost == boost)
        .ok_or_else(|| {
            anyhow!(
                "gain model '{}' does not support mode='{}' boost={}",
                MODEL_ID, mode, boost
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
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
