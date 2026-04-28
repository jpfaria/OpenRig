use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_prs_mt100_tremonti";
pub const DISPLAY_NAME: &str = "MT-100 Tremonti";
const BRAND: &str = "prs";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voicing × mic.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, mic, file)
    ("clean_cleanly",      "di",   "amps/prs_mt100_tremonti/amp_prs_mt100_clean_cleanly_di_std.nam"),
    ("clean_noon",         "di",   "amps/prs_mt100_tremonti/amp_prs_mt100_clean_noon_di_std.nam"),
    ("clean_noon",         "sm57", "amps/prs_mt100_tremonti/amp_prs_mt100_clean_noon_sm57_std.nam"),
    ("clean_live_2023",    "di",   "amps/prs_mt100_tremonti/amp_prs_mt100_clean_live_2023_di_std.nam"),
    ("clean_live_2023",    "sm57", "amps/prs_mt100_tremonti/amp_prs_mt100_clean_live_2023_sm57_std.nam"),
    ("od_dime_it",         "sm57", "amps/prs_mt100_tremonti/amp_prs_mt100_overdrive_dime_it_sm57.nam"),
    ("od_bitey",           "di",   "amps/prs_mt100_tremonti/amp_prs_mt100_overdrive_bitey_di_std.nam"),
    ("od_sin_after_sin",   "di",   "amps/prs_mt100_tremonti/amp_prs_mt100_overdrive_sin_after_sin_di_std.nam"),
    ("od_sin_after_sin",   "sm57", "amps/prs_mt100_tremonti/amp_prs_mt100_overdrive_sin_after_sin_sm57_std.nam"),
    ("od_sc_leads",        "sm57", "amps/prs_mt100_tremonti/amp_prs_mt100_overdrive_single_coil_leads_sm57_std.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("clean_noon"),
            &[
                ("clean_cleanly",    "Clean Cleanly"),
                ("clean_noon",       "Clean Noon"),
                ("clean_live_2023",  "Clean Live 2023"),
                ("od_dime_it",       "OD Dime It"),
                ("od_bitey",         "OD Bitey"),
                ("od_sin_after_sin", "OD Sin After Sin"),
                ("od_sc_leads",      "OD SC Leads"),
            ],
        ),
        enum_parameter(
            "mic",
            "Mic",
            Some("Amp"),
            Some("sm57"),
            &[
                ("di",   "DI (No Cab)"),
                ("sm57", "SM57"),
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
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, m, _)| *v == voicing && *m == mic)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} mic={}",
                MODEL_ID, voicing, mic
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
