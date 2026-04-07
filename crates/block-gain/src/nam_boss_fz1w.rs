use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "boss_fz1w";
pub const DISPLAY_NAME: &str = "FZ-1W Fuzz";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fz1wParams {
    pub mode: &'static str,
    pub fuzz: &'static str,
}

struct Fz1wCapture {
    params: Fz1wParams,
    model_path: &'static str,
}

const CAPTURES: &[Fz1wCapture] = &[
    Fz1wCapture { params: Fz1wParams { mode: "modern",  fuzz: "2"  }, model_path: "pedals/boss_fz1w/fz1w_modern_f02.nam" },
    Fz1wCapture { params: Fz1wParams { mode: "modern",  fuzz: "5"  }, model_path: "pedals/boss_fz1w/fz1w_modern_f05.nam" },
    Fz1wCapture { params: Fz1wParams { mode: "modern",  fuzz: "7"  }, model_path: "pedals/boss_fz1w/fz1w_modern_f07.nam" },
    Fz1wCapture { params: Fz1wParams { mode: "modern",  fuzz: "11" }, model_path: "pedals/boss_fz1w/fz1w_modern_f11.nam" },
    Fz1wCapture { params: Fz1wParams { mode: "vintage", fuzz: "2"  }, model_path: "pedals/boss_fz1w/fz1w_vintage_f02.nam" },
    Fz1wCapture { params: Fz1wParams { mode: "vintage", fuzz: "5"  }, model_path: "pedals/boss_fz1w/fz1w_vintage_f05.nam" },
    Fz1wCapture { params: Fz1wParams { mode: "vintage", fuzz: "7"  }, model_path: "pedals/boss_fz1w/fz1w_vintage_f07.nam" },
    Fz1wCapture { params: Fz1wParams { mode: "vintage", fuzz: "11" }, model_path: "pedals/boss_fz1w/fz1w_vintage_f11.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "mode",
            "Mode",
            Some("Pedal"),
            Some("modern"),
            &[("modern", "Modern"), ("vintage", "Vintage")],
        ),
        enum_parameter(
            "fuzz",
            "Fuzz",
            Some("Pedal"),
            Some("5"),
            &[("2", "2"), ("5", "5"), ("7", "7"), ("11", "11")],
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static Fz1wCapture> {
    let mode = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    let fuzz = required_string(params, "fuzz").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.params.mode == mode && c.params.fuzz == fuzz)
        .ok_or_else(|| {
            anyhow!(
                "gain model '{}' does not support mode='{}' fuzz='{}'",
                MODEL_ID, mode, fuzz
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
