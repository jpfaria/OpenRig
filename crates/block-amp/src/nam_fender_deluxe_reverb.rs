use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fender_deluxe_reverb";
pub const DISPLAY_NAME: &str = "Deluxe Reverb";
const BRAND: &str = "fender";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: version × mic-and-room.
// "DI" capture only available on the original version (no cab).
const CAPTURES: &[(&str, &str, &str)] = &[
    // (version, capture_kind, file)
    ("original", "di_no_cab",        "amps/fender_deluxe_reverb/fender_drri_clean_di_capture_no_cab.nam"),
    ("original", "room_only",        "amps/fender_deluxe_reverb/fender_drri_clean_room_only_full_rig.nam"),
    ("original", "sm57_royer_dry",   "amps/fender_deluxe_reverb/fender_drri_clean_sm57_royer_r_121_no_room_full_rig.nam"),
    ("original", "sm57_royer_room",  "amps/fender_deluxe_reverb/fender_drri_clean_sm57_royer_r_121_room_full_rig.nam"),
    ("new",      "room_only",        "amps/fender_deluxe_reverb/new_version_fender_drri_clean_room_only_full_rig.nam"),
    ("new",      "sm57_royer_dry",   "amps/fender_deluxe_reverb/new_version_fender_drri_clean_sm57_royer_r_121_no_room_full_.nam"),
    ("new",      "sm57_royer_room",  "amps/fender_deluxe_reverb/new_version_fender_drri_clean_sm57_royer_r_121_room_full_rig.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "version",
            "Version",
            Some("Amp"),
            Some("original"),
            &[
                ("original", "Original"),
                ("new",      "New"),
            ],
        ),
        enum_parameter(
            "capture_kind",
            "Capture Kind",
            Some("Amp"),
            Some("sm57_royer_dry"),
            &[
                ("di_no_cab",       "DI (No Cab)"),
                ("room_only",       "Room Only"),
                ("sm57_royer_dry",  "SM57 + Royer (Dry)"),
                ("sm57_royer_room", "SM57 + Royer + Room"),
            ],
        ),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let path = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let version = required_string(params, "version").map_err(anyhow::Error::msg)?;
    let kind = required_string(params, "capture_kind").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, k, _)| *v == version && *k == kind)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for version={} capture_kind={}",
                MODEL_ID, version, kind
            )
        })
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
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

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}
