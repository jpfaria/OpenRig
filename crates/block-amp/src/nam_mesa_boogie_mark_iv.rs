use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_boogie_mark_iv";
pub const DISPLAY_NAME: &str = "Mark IV";
const BRAND: &str = "mesa";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voicing × take.
// Voicings group by tone family; take = rhythm/lead/single capture per voicing.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, take, file)
    ("tight_iic",      "rhythm_1", "amps/mesa_boogie_mark_iv/kdm_slammin_mkiv_28_tight_iic_rhythm_1_s.nam"),
    ("tight_iic",      "rhythm_2", "amps/mesa_boogie_mark_iv/kdm_slammin_mkiv_29_tight_iic_rhythm_2_s.nam"),
    ("fat_iic",        "rhythm_1", "amps/mesa_boogie_mark_iv/kdm_slammin_mkiv_31_fat_iic_rhythm_1_s.nam"),
    ("fat_iic",        "rhythm_2", "amps/mesa_boogie_mark_iv/kdm_slammin_mkiv_32_fat_iic_rhythm_2_s.nam"),
    ("petrucci",       "crunch",   "amps/mesa_boogie_mark_iv/kdm_slammin_mkiv_34_petrucci_mark_iv_crunch_s.nam"),
    ("metallica_85",   "notes",    "amps/mesa_boogie_mark_iv/kdm_slammin_mkiv_37_metallica_85_notes_s.nam"),
    ("metallica_tba",  "default",  "amps/mesa_boogie_mark_iv/kdm_slammin_mkiv_39_metallica_tba_s.nam"),
    ("log_sacrament",  "default",  "amps/mesa_boogie_mark_iv/kdm_slammin_mkiv_41_log_sacrament_s.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("tight_iic"),
            &[
                ("tight_iic",     "Tight IIC+"),
                ("fat_iic",       "Fat IIC+"),
                ("petrucci",      "Petrucci"),
                ("metallica_85",  "Metallica '85"),
                ("metallica_tba", "Metallica TBA"),
                ("log_sacrament", "LOG Sacrament"),
            ],
        ),
        enum_parameter(
            "take",
            "Take",
            Some("Amp"),
            Some("rhythm_1"),
            &[
                ("rhythm_1", "Rhythm 1"),
                ("rhythm_2", "Rhythm 2"),
                ("crunch",   "Crunch"),
                ("notes",    "Notes"),
                ("default",  "Default"),
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
    let voicing = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    let take = required_string(params, "take").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, t, _)| *v == voicing && *t == take)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} take={}",
                MODEL_ID, voicing, take
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
