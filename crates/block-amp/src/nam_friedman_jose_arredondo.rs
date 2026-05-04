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

// Two-axis pack: voicing × mic.
// Voicings: Mesa 4x12 V.Pres @5, Marshall 4x12 N.Pres @5, Marshall Hotrod.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, mic, file)
    ("mesa_v_pres",   "sm57",   "amps/friedman_jose_arredondo/pow_fman_jose_mes4x12_v_pres_pres_5_sm57.nam"),
    ("mesa_v_pres",   "blend1", "amps/friedman_jose_arredondo/pow_fman_jose_mes4x12_v_pres_pres_5_blend_1.nam"),
    ("mar_n_pres",    "sm57",   "amps/friedman_jose_arredondo/pow_fman_jose_mar4x12_n_pres_pres_5_sm57.nam"),
    ("mar_n_pres",    "di",     "amps/friedman_jose_arredondo/pow_fman_jose_mar4x12_n_pres_pres_5_di.nam"),
    ("mar_n_pres",    "blend1", "amps/friedman_jose_arredondo/pow_fman_jose_mar4x12_n_pres_pres_5_blend_1.nam"),
    ("mar_hotrod",    "sm57",   "amps/friedman_jose_arredondo/amp_fman_jose_mar4x12_n_pres_hotrod_sm57.nam"),
    ("mar_hotrod",    "blend1", "amps/friedman_jose_arredondo/amp_fman_jose_mar4x12_n_pres_hotrod_blend_1.nam"),
    ("mar_hotrod",    "blend3", "amps/friedman_jose_arredondo/amp_fman_jose_mar4x12_n_pres_hotrod_blend_3.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("mar_n_pres"),
            &[
                ("mesa_v_pres", "Mesa 4x12 V.Pres"),
                ("mar_n_pres",  "Marshall 4x12 N.Pres"),
                ("mar_hotrod",  "Marshall Hotrod"),
            ],
        ),
        enum_parameter(
            "mic",
            "Mic",
            Some("Amp"),
            Some("sm57"),
            &[
                ("sm57",   "SM57"),
                ("di",     "DI (No Cab)"),
                ("blend1", "Blend #1"),
                ("blend3", "Blend #3"),
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
