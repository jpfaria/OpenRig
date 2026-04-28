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

const CAPTURES: &[(&str, &str, &str)] = &[
    ("sm57_royer_r_121_no_room_full_rig", "Fender DRRI | Clean | SM57 + Royer R-121 (No Room) | Full Ri", "amps/fender_deluxe_reverb/fender_drri_clean_sm57_royer_r_121_no_room_full_rig_2.nam"),
    ("room_only_full_rig", "Fender DRRI | Clean | Room Only | Full Rig", "amps/fender_deluxe_reverb/fender_drri_clean_room_only_full_rig_2.nam"),
    ("sm57_royer_r_121_room_full_rig", "Fender DRRI | Clean | SM57 + Royer R-121 + Room | Full Rig", "amps/fender_deluxe_reverb/fender_drri_clean_sm57_royer_r_121_room_full_rig_2.nam"),
    ("new_version_room_only_full_rig", "NEW VERSION | Fender DRRI | Clean | Room Only | Full Rig", "amps/fender_deluxe_reverb/new_version_fender_drri_clean_room_only_full_rig_2.nam"),
    ("new_version_sm57_royer_r_121_room_full_r", "NEW VERSION | Fender DRRI | Clean | SM57 + Royer R-121 + Roo", "amps/fender_deluxe_reverb/new_version_fender_drri_clean_sm57_royer_r_121_room_full_rig_2.nam"),
    ("new_version_sm57_royer_r_121_no_room_ful", "NEW VERSION | Fender DRRI | Clean | SM57 + Royer R-121 (No R", "amps/fender_deluxe_reverb/new_version_fender_drri_clean_sm57_royer_r_121_no_room_full__2.nam"),
    ("di_capture_no_cab", "Fender DRRI | Clean | DI Capture (No Cab)", "amps/fender_deluxe_reverb/fender_drri_clean_di_capture_no_cab_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("sm57_royer_r_121_no_room_full_rig"),
        &[
            ("sm57_royer_r_121_no_room_full_rig", "Fender DRRI | Clean | SM57 + Royer R-121 (No Room) | Full Ri"),
            ("room_only_full_rig", "Fender DRRI | Clean | Room Only | Full Rig"),
            ("sm57_royer_r_121_room_full_rig", "Fender DRRI | Clean | SM57 + Royer R-121 + Room | Full Rig"),
            ("new_version_room_only_full_rig", "NEW VERSION | Fender DRRI | Clean | Room Only | Full Rig"),
            ("new_version_sm57_royer_r_121_room_full_r", "NEW VERSION | Fender DRRI | Clean | SM57 + Royer R-121 + Roo"),
            ("new_version_sm57_royer_r_121_no_room_ful", "NEW VERSION | Fender DRRI | Clean | SM57 + Royer R-121 (No R"),
            ("di_capture_no_cab", "Fender DRRI | Clean | DI Capture (No Cab)"),
        ],
    )];
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
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no capture '{}'", MODEL_ID, key))
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
