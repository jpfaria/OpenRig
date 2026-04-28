use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_friedman_jose_arredondo";
pub const DISPLAY_NAME: &str = "Jose Arredondo";
const BRAND: &str = "friedman";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("pow_fman_jose_mes4x12_v_pres_pres_5_blen", "[POW] FMAN.JOSE-Mes4x12-V.PRES Pres@5 - BLEND #1", "amps/friedman_jose_arredondo/pow_fman_jose_mes4x12_v_pres_pres_5_blend_1_2.nam"),
    ("pow_fman_jose_mes4x12_v_pres_pres_5_sm57", "[POW] FMAN.JOSE-Mes4x12-V.PRES Pres@5 - SM57", "amps/friedman_jose_arredondo/pow_fman_jose_mes4x12_v_pres_pres_5_sm57_2.nam"),
    ("pow_fman_jose_mar4x12_n_pres_pres_5_sm57", "[POW] FMAN.JOSE-Mar4x12-N.PRES Pres@5 - SM57", "amps/friedman_jose_arredondo/pow_fman_jose_mar4x12_n_pres_pres_5_sm57_2.nam"),
    ("pow_fman_jose_mar4x12_n_pres_pres_5_di", "[POW] FMAN.JOSE-Mar4x12-N.PRES Pres@5 - DI", "amps/friedman_jose_arredondo/pow_fman_jose_mar4x12_n_pres_pres_5_di_2.nam"),
    ("pow_fman_jose_mar4x12_n_pres_pres_5_blen", "[POW] FMAN.JOSE-Mar4x12-N.PRES Pres@5 - BLEND #1", "amps/friedman_jose_arredondo/pow_fman_jose_mar4x12_n_pres_pres_5_blend_1_2.nam"),
    ("amp_fman_jose_mar4x12_n_pres_hotrod_blen", "[AMP] FMAN.JOSE-Mar4x12-N.PRES Hotrod - BLEND #1", "amps/friedman_jose_arredondo/amp_fman_jose_mar4x12_n_pres_hotrod_blend_1_2.nam"),
    ("amp_fman_jose_mar4x12_n_pres_hotrod_blen_336414", "[AMP] FMAN.JOSE-Mar4x12-N.PRES Hotrod - BLEND #3", "amps/friedman_jose_arredondo/amp_fman_jose_mar4x12_n_pres_hotrod_blend_3_2.nam"),
    ("amp_fman_jose_mar4x12_n_pres_hotrod_sm57", "[AMP] FMAN.JOSE-Mar4x12-N.PRES Hotrod - SM57", "amps/friedman_jose_arredondo/amp_fman_jose_mar4x12_n_pres_hotrod_sm57_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("pow_fman_jose_mes4x12_v_pres_pres_5_blen"),
        &[
            ("pow_fman_jose_mes4x12_v_pres_pres_5_blen", "[POW] FMAN.JOSE-Mes4x12-V.PRES Pres@5 - BLEND #1"),
            ("pow_fman_jose_mes4x12_v_pres_pres_5_sm57", "[POW] FMAN.JOSE-Mes4x12-V.PRES Pres@5 - SM57"),
            ("pow_fman_jose_mar4x12_n_pres_pres_5_sm57", "[POW] FMAN.JOSE-Mar4x12-N.PRES Pres@5 - SM57"),
            ("pow_fman_jose_mar4x12_n_pres_pres_5_di", "[POW] FMAN.JOSE-Mar4x12-N.PRES Pres@5 - DI"),
            ("pow_fman_jose_mar4x12_n_pres_pres_5_blen", "[POW] FMAN.JOSE-Mar4x12-N.PRES Pres@5 - BLEND #1"),
            ("amp_fman_jose_mar4x12_n_pres_hotrod_blen", "[AMP] FMAN.JOSE-Mar4x12-N.PRES Hotrod - BLEND #1"),
            ("amp_fman_jose_mar4x12_n_pres_hotrod_blen_336414", "[AMP] FMAN.JOSE-Mar4x12-N.PRES Hotrod - BLEND #3"),
            ("amp_fman_jose_mar4x12_n_pres_hotrod_sm57", "[AMP] FMAN.JOSE-Mar4x12-N.PRES Hotrod - SM57"),
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
