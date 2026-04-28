use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_friedman_be100";
pub const DISPLAY_NAME: &str = "BE 100";
const BRAND: &str = "friedman";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("amp_be100dlx_hbe_mammoth_di_std", "[AMP] BE100DLX-HBE Mammoth DI - STD", "amps/friedman_be100/amp_be100dlx_hbe_mammoth_di_std.nam"),
    ("amp_be100dlx_cln_tender_clean_di", "[AMP] BE100DLX-CLN Tender Clean DI - STD", "amps/friedman_be100/amp_be100dlx_cln_tender_clean_di_std.nam"),
    ("amp_be100dlx_hbe_tallica_di_std", "[AMP] BE100DLX-HBE Tallica DI - STD", "amps/friedman_be100/amp_be100dlx_hbe_tallica_di_std.nam"),
    ("amp_be100dlx_hbe_tallica_sm57_st", "[AMP] BE100DLX-HBE Tallica SM57 - STD", "amps/friedman_be100/amp_be100dlx_hbe_tallica_sm57_std.nam"),
    ("amp_be100dlx_hbe_tallica_sm58_st", "[AMP] BE100DLX-HBE Tallica SM58 - STD", "amps/friedman_be100/amp_be100dlx_hbe_tallica_sm58_std.nam"),
    ("amp_be100dlx_cln_tender_clean_sm", "[AMP] BE100DLX-CLN Tender Clean SM57 - STD", "amps/friedman_be100/amp_be100dlx_cln_tender_clean_sm57_std.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("amp_be100dlx_hbe_mammoth_di_std"),
        &[
            ("amp_be100dlx_hbe_mammoth_di_std", "[AMP] BE100DLX-HBE Mammoth DI - STD"),
            ("amp_be100dlx_cln_tender_clean_di", "[AMP] BE100DLX-CLN Tender Clean DI - STD"),
            ("amp_be100dlx_hbe_tallica_di_std", "[AMP] BE100DLX-HBE Tallica DI - STD"),
            ("amp_be100dlx_hbe_tallica_sm57_st", "[AMP] BE100DLX-HBE Tallica SM57 - STD"),
            ("amp_be100dlx_hbe_tallica_sm58_st", "[AMP] BE100DLX-HBE Tallica SM58 - STD"),
            ("amp_be100dlx_cln_tender_clean_sm", "[AMP] BE100DLX-CLN Tender Clean SM57 - STD"),
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
