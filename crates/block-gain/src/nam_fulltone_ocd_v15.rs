use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "fulltone_ocd_v15";
pub const DISPLAY_NAME: &str = "OCD v1.5";
const BRAND: &str = "fulltone";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcdV15Params {
    pub mode: &'static str,
    pub drive: &'static str,
}

struct OcdV15Capture {
    params: OcdV15Params,
    model_path: &'static str,
}

const CAPTURES: &[OcdV15Capture] = &[
    OcdV15Capture { params: OcdV15Params { mode: "lp", drive: "3"  }, model_path: "pedals/fulltone_ocd_v15/ocd15_lp_d3.nam" },
    OcdV15Capture { params: OcdV15Params { mode: "lp", drive: "9"  }, model_path: "pedals/fulltone_ocd_v15/ocd15_lp_d9.nam" },
    OcdV15Capture { params: OcdV15Params { mode: "lp", drive: "12" }, model_path: "pedals/fulltone_ocd_v15/ocd15_lp_d12.nam" },
    OcdV15Capture { params: OcdV15Params { mode: "hp", drive: "3"  }, model_path: "pedals/fulltone_ocd_v15/ocd15_hp_d3.nam" },
    OcdV15Capture { params: OcdV15Params { mode: "hp", drive: "9"  }, model_path: "pedals/fulltone_ocd_v15/ocd15_hp_d9.nam" },
    OcdV15Capture { params: OcdV15Params { mode: "hp", drive: "12" }, model_path: "pedals/fulltone_ocd_v15/ocd15_hp_d12.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "mode",
            "Mode",
            Some("Pedal"),
            Some("lp"),
            &[("lp", "LP"), ("hp", "HP")],
        ),
        enum_parameter(
            "drive",
            "Drive",
            Some("Pedal"),
            Some("9"),
            &[("3", "Low"), ("9", "Medium"), ("12", "High")],
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static OcdV15Capture> {
    let mode = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    let drive = required_string(params, "drive").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.params.mode == mode && c.params.drive == drive)
        .ok_or_else(|| {
            anyhow!(
                "gain model '{}' does not support mode='{}' drive='{}'",
                MODEL_ID, mode, drive
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
