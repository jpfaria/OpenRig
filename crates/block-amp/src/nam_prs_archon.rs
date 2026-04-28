use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_prs_archon";
pub const DISPLAY_NAME: &str = "Archon";
const BRAND: &str = "prs";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("amp_arc50c_lead_hippo", "[AMP] ARC50C-LEAD Hippo - STD", "amps/prs_archon/amp_arc50c_lead_hippo_std.nam"),
    ("amp_arc50c_lead_roar", "[AMP] ARC50C-LEAD Roar - STD", "amps/prs_archon/amp_arc50c_lead_roar_std.nam"),
    ("amp_arc50c_lead_rhino", "[AMP] ARC50C-LEAD Rhino - STD", "amps/prs_archon/amp_arc50c_lead_rhino_std.nam"),
    ("amp_arc50c_lead_growl", "[AMP] ARC50C-LEAD Growl - STD", "amps/prs_archon/amp_arc50c_lead_growl_std.nam"),
    ("amp_arc50c_lead_nessie", "[AMP] ARC50C-LEAD Nessie - STD", "amps/prs_archon/amp_arc50c_lead_nessie_std.nam"),
    ("amp_arc50c_lead_kong", "[AMP] ARC50C-LEAD Kong - STD", "amps/prs_archon/amp_arc50c_lead_kong_std.nam"),
    ("pow_arc50c_p5_d5_cl_mesa4x12", "[POW] ARC50C P5 D5 - CL (Mesa4x12) - STD", "amps/prs_archon/pow_arc50c_p5_d5_cl_mesa4x12_std.nam"),
    ("pow_arc50c_p8_d2_rl", "[POW] ARC50C P8 D2 - RL - STD", "amps/prs_archon/pow_arc50c_p8_d2_rl_std.nam"),
    ("pow_arc50c_p6_d6_rl", "[POW] ARC50C P6 D6 - RL - STD", "amps/prs_archon/pow_arc50c_p6_d6_rl_std.nam"),
    ("pow_arc50c_p2_d6_rl", "[POW] ARC50C P2 D6 - RL - STD", "amps/prs_archon/pow_arc50c_p2_d6_rl_std.nam"),
    ("pow_arc50c_p5_d2_rl", "[POW] ARC50C P5 D2 - RL - STD", "amps/prs_archon/pow_arc50c_p5_d2_rl_std.nam"),
    ("pow_arc50c_p2_d8_rl", "[POW] ARC50C P2 D8 - RL - STD", "amps/prs_archon/pow_arc50c_p2_d8_rl_std.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("amp_arc50c_lead_hippo"),
        &[
            ("amp_arc50c_lead_hippo", "[AMP] ARC50C-LEAD Hippo - STD"),
            ("amp_arc50c_lead_roar", "[AMP] ARC50C-LEAD Roar - STD"),
            ("amp_arc50c_lead_rhino", "[AMP] ARC50C-LEAD Rhino - STD"),
            ("amp_arc50c_lead_growl", "[AMP] ARC50C-LEAD Growl - STD"),
            ("amp_arc50c_lead_nessie", "[AMP] ARC50C-LEAD Nessie - STD"),
            ("amp_arc50c_lead_kong", "[AMP] ARC50C-LEAD Kong - STD"),
            ("pow_arc50c_p5_d5_cl_mesa4x12", "[POW] ARC50C P5 D5 - CL (Mesa4x12) - STD"),
            ("pow_arc50c_p8_d2_rl", "[POW] ARC50C P8 D2 - RL - STD"),
            ("pow_arc50c_p6_d6_rl", "[POW] ARC50C P6 D6 - RL - STD"),
            ("pow_arc50c_p2_d6_rl", "[POW] ARC50C P2 D6 - RL - STD"),
            ("pow_arc50c_p5_d2_rl", "[POW] ARC50C P5 D2 - RL - STD"),
            ("pow_arc50c_p2_d8_rl", "[POW] ARC50C P2 D8 - RL - STD"),
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
