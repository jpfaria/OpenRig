use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};

use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "peavey_5150_mesa_4x12";
pub const DISPLAY_NAME: &str = "5150 + Mesa 4\u{00d7}12";
const BRAND: &str = "peavey";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peavey5150Params {
    pub boost: &'static str,
    pub mic: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peavey5150Capture {
    pub params: Peavey5150Params,
    pub model_path: &'static str,
}

// Note: no_boost only has sm57. If sm58 + no_boost is requested, falls back to sm57.
pub const CAPTURES: &[Peavey5150Capture] = &[
    capture("no_boost", "sm57", "full_rigs/peavey_5150_mesa_4x12/peavey_5150_mesa_no_boost_sm57.nam"),
    capture("maxon",    "sm57", "full_rigs/peavey_5150_mesa_4x12/peavey_5150_mesa_maxon_sm57.nam"),
    capture("maxon",    "sm58", "full_rigs/peavey_5150_mesa_4x12/peavey_5150_mesa_maxon_sm58.nam"),
    capture("mxr",      "sm57", "full_rigs/peavey_5150_mesa_4x12/peavey_5150_mesa_mxr_sm57.nam"),
    capture("mxr",      "sm58", "full_rigs/peavey_5150_mesa_4x12/peavey_5150_mesa_mxr_sm58.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "boost",
            "Boost",
            Some("Amp"),
            Some("no_boost"),
            &[
                ("no_boost", "No Boost"),
                ("maxon", "Maxon OD808"),
                ("mxr", "MXR"),
            ],
        ),
        enum_parameter(
            "mic",
            "Mic",
            Some("Cab"),
            Some("sm57"),
            &[("sm57", "SM57"), ("sm58", "SM58")],
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static Peavey5150Capture> {
    let boost = required_string(params, "boost").map_err(anyhow::Error::msg)?;
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;

    // Exact match first
    if let Some(c) = CAPTURES.iter().find(|c| c.params.boost == boost && c.params.mic == mic) {
        return Ok(c);
    }

    // Fallback: no_boost + sm58 -> use sm57
    if boost == "no_boost" && mic == "sm58" {
        if let Some(c) = CAPTURES.iter().find(|c| c.params.boost == "no_boost" && c.params.mic == "sm57") {
            return Ok(c);
        }
    }

    Err(anyhow!(
        "amp model '{}' does not support boost='{}' mic='{}'",
        MODEL_ID,
        boost,
        mic
    ))
}

const fn capture(boost: &'static str, mic: &'static str, model_path: &'static str) -> Peavey5150Capture {
    Peavey5150Capture {
        params: Peavey5150Params { boost, mic },
        model_path,
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: AmpBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
