use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};

use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "fender_deluxe_reverb_65";
pub const DISPLAY_NAME: &str = "Deluxe Reverb '65";
const BRAND: &str = "fender";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURE_SM57_ROYER: &str = "full_rigs/fender_deluxe_reverb_65/fender_drri_clean_sm57_royer.nam";
const CAPTURE_SM57_ROYER_ROOM: &str = "full_rigs/fender_deluxe_reverb_65/fender_drri_clean_sm57_royer_room.nam";
const CAPTURE_ROOM: &str = "full_rigs/fender_deluxe_reverb_65/fender_drri_clean_room.nam";

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "mic",
        "Mic",
        Some("Cab"),
        Some("sm57_royer"),
        &[
            ("sm57_royer", "SM57 + Royer R-121"),
            ("sm57_royer_room", "SM57 + Royer R-121 + Room"),
            ("room", "Room Only"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let model_path = resolve_capture_path(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture_path(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture_path(params)?;
    Ok(format!("model='{}'", path))
}

fn resolve_capture_path(params: &ParameterSet) -> Result<&'static str> {
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;
    match mic.as_str() {
        "sm57_royer" => Ok(CAPTURE_SM57_ROYER),
        "sm57_royer_room" => Ok(CAPTURE_SM57_ROYER_ROOM),
        "room" => Ok(CAPTURE_ROOM),
        other => Err(anyhow!(
            "amp model '{}' does not support mic='{}'",
            MODEL_ID,
            other
        )),
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
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS,
    knob_layout: &[],
};
