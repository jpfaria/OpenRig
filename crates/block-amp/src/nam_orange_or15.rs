use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_orange_or15";
pub const DISPLAY_NAME: &str = "OR15";
const BRAND: &str = "orange";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("orange_or15_clean_with_foxpedal__feather", "Orange OR15 (Clean with Foxpedal Defector Fuzz Rasputin Mode", "amps/orange_or15/orange_or15_clean_with_foxpedal_defector_fuzz_rasputin_mode_feather.nam"),
    ("orange_or15_clean_with_boss_hm_2_feather", "Orange OR15 (Clean with Boss HM-2w Heavy Metal Pedal)", "amps/orange_or15/orange_or15_clean_with_boss_hm_2w_heavy_metal_pedal_feather.nam"),
    ("orange_or15_clean_with_tc_electr_feather", "Orange OR15 (Clean with TC Electronic Eyemaster)", "amps/orange_or15/orange_or15_clean_with_tc_electronic_eyemaster_feather.nam"),
    ("orange_or15_crunch_with_eea_mud__feather", "Orange OR15 (Crunch with EEA Mud Killer Fat Boost)", "amps/orange_or15/orange_or15_crunch_with_eea_mud_killer_fat_boost_feather.nam"),
    ("orange_or15_crunch_with_eea_mudk_feather", "Orange OR15 (Crunch with EEA Mudkiller into EHX Green Russia", "amps/orange_or15/orange_or15_crunch_with_eea_mudkiller_into_ehx_green_russian_feather.nam"),
    ("orange_or15_clean_with_foxpedal__feather_50323", "Orange OR15 (Clean with Foxpedal Defector Fuzz)", "amps/orange_or15/orange_or15_clean_with_foxpedal_defector_fuzz_feather.nam"),
    ("orange_or15_clean_with_behringer_feather", "Orange OR15 (Clean with Behringer SF300 Super Fuzz Mode 1.5)", "amps/orange_or15/orange_or15_clean_with_behringer_sf300_super_fuzz_mode_1_5_1_feather.nam"),
    ("orange_or15_clean_with_behringer_feather_50319", "Orange OR15 (Clean with Behringer SF300 Super Fuzz Mode 1.5)", "amps/orange_or15/orange_or15_clean_with_behringer_sf300_super_fuzz_mode_1_5_feather.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("orange_or15_clean_with_foxpedal__feather"),
        &[
            ("orange_or15_clean_with_foxpedal__feather", "Orange OR15 (Clean with Foxpedal Defector Fuzz Rasputin Mode"),
            ("orange_or15_clean_with_boss_hm_2_feather", "Orange OR15 (Clean with Boss HM-2w Heavy Metal Pedal)"),
            ("orange_or15_clean_with_tc_electr_feather", "Orange OR15 (Clean with TC Electronic Eyemaster)"),
            ("orange_or15_crunch_with_eea_mud__feather", "Orange OR15 (Crunch with EEA Mud Killer Fat Boost)"),
            ("orange_or15_crunch_with_eea_mudk_feather", "Orange OR15 (Crunch with EEA Mudkiller into EHX Green Russia"),
            ("orange_or15_clean_with_foxpedal__feather_50323", "Orange OR15 (Clean with Foxpedal Defector Fuzz)"),
            ("orange_or15_clean_with_behringer_feather", "Orange OR15 (Clean with Behringer SF300 Super Fuzz Mode 1.5)"),
            ("orange_or15_clean_with_behringer_feather_50319", "Orange OR15 (Clean with Behringer SF300 Super Fuzz Mode 1.5)"),
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
