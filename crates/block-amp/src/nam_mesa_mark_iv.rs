use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_mark_iv";
pub const DISPLAY_NAME: &str = "Mark IV";
const BRAND: &str = "mesa";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("kdm_slammin_mkiv_28_tight_iic_rhythm_1", "KDM-Slammin_MKIV_28 Tight IIC+ Rhythm 1 S", "amps/mesa_mark_iv/kdm_slammin_mkiv_28_tight_iic_rhythm_1_s_2.nam"),
    ("kdm_slammin_mkiv_31_fat_iic_rhythm_1", "KDM-Slammin_MKIV_31 Fat IIC+ Rhythm 1  S", "amps/mesa_mark_iv/kdm_slammin_mkiv_31_fat_iic_rhythm_1_s_2.nam"),
    ("kdm_slammin_mkiv_37_metallica_85_notes", "KDM-Slammin_MKIV_37 Metallica '85 Notes S", "amps/mesa_mark_iv/kdm_slammin_mkiv_37_metallica_85_notes_s_2.nam"),
    ("kdm_slammin_mkiv_29_tight_iic_rhythm_2", "KDM-Slammin_MKIV_29 Tight IIC+ Rhythm 2  S", "amps/mesa_mark_iv/kdm_slammin_mkiv_29_tight_iic_rhythm_2_s_2.nam"),
    ("kdm_slammin_mkiv_32_fat_iic_rhythm_2", "KDM-Slammin_MKIV_32 Fat IIC+ Rhythm 2  S", "amps/mesa_mark_iv/kdm_slammin_mkiv_32_fat_iic_rhythm_2_s_2.nam"),
    ("kdm_slammin_mkiv_39_metallica_tba", "KDM-Slammin_MKIV_39 Metallica TBA  S", "amps/mesa_mark_iv/kdm_slammin_mkiv_39_metallica_tba_s_2.nam"),
    ("kdm_slammin_mkiv_34_petrucci_mark_iv_cru", "KDM-Slammin_MKIV_34 Petrucci Mark IV Crunch S", "amps/mesa_mark_iv/kdm_slammin_mkiv_34_petrucci_mark_iv_crunch_s_2.nam"),
    ("kdm_slammin_mkiv_41_log_sacrament", "KDM-Slammin_MKIV_41 LOG Sacrament S", "amps/mesa_mark_iv/kdm_slammin_mkiv_41_log_sacrament_s_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("kdm_slammin_mkiv_28_tight_iic_rhythm_1"),
        &[
            ("kdm_slammin_mkiv_28_tight_iic_rhythm_1", "KDM-Slammin_MKIV_28 Tight IIC+ Rhythm 1 S"),
            ("kdm_slammin_mkiv_31_fat_iic_rhythm_1", "KDM-Slammin_MKIV_31 Fat IIC+ Rhythm 1  S"),
            ("kdm_slammin_mkiv_37_metallica_85_notes", "KDM-Slammin_MKIV_37 Metallica '85 Notes S"),
            ("kdm_slammin_mkiv_29_tight_iic_rhythm_2", "KDM-Slammin_MKIV_29 Tight IIC+ Rhythm 2  S"),
            ("kdm_slammin_mkiv_32_fat_iic_rhythm_2", "KDM-Slammin_MKIV_32 Fat IIC+ Rhythm 2  S"),
            ("kdm_slammin_mkiv_39_metallica_tba", "KDM-Slammin_MKIV_39 Metallica TBA  S"),
            ("kdm_slammin_mkiv_34_petrucci_mark_iv_cru", "KDM-Slammin_MKIV_34 Petrucci Mark IV Crunch S"),
            ("kdm_slammin_mkiv_41_log_sacrament", "KDM-Slammin_MKIV_41 LOG Sacrament S"),
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
