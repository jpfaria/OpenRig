use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_friedman_be_100";
pub const DISPLAY_NAME: &str = "BE 100";
const BRAND: &str = "friedman";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voicing × mic.
// "noon_cln" is preamp-only ([PRE] capture, DI). Others are full amp captures.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, mic, file)
    ("hbe_mammoth",  "di",   "amps/friedman_be_100/amp_be100dlx_hbe_mammoth_di_std.nam"),
    ("hbe_mammoth",  "sm57", "amps/friedman_be_100/amp_be100dlx_hbe_mammoth_sm57_std.nam"),
    ("hbe_tallica",  "di",   "amps/friedman_be_100/amp_be100dlx_hbe_tallica_di_std.nam"),
    ("hbe_tallica",  "sm57", "amps/friedman_be_100/amp_be100dlx_hbe_tallica_sm57_std.nam"),
    ("hbe_tallica",  "sm58", "amps/friedman_be_100/amp_be100dlx_hbe_tallica_sm58_std.nam"),
    ("cln_tender",   "di",   "amps/friedman_be_100/amp_be100dlx_cln_tender_clean_di_std.nam"),
    ("cln_tender",   "sm57", "amps/friedman_be_100/amp_be100dlx_cln_tender_clean_sm57_std.nam"),
    ("cln_noon_pre", "di",   "amps/friedman_be_100/pre_be100dlx_cln_noon_cln_03_std.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("hbe_mammoth"),
            &[
                ("hbe_mammoth",  "HBE Mammoth"),
                ("hbe_tallica",  "HBE Tallica"),
                ("cln_tender",   "CLN Tender"),
                ("cln_noon_pre", "CLN Noon (Preamp)"),
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
                ("sm58", "SM58"),
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
