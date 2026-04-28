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

// Two-axis pack: stage × tone preset.
// stage = AMP (full amp) / POW (power amp). All ARC50C captures.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (stage, tone, file)
    ("amp", "lead_hippo",  "amps/prs_archon/amp_arc50c_lead_hippo_std.nam"),
    ("amp", "lead_roar",   "amps/prs_archon/amp_arc50c_lead_roar_std.nam"),
    ("amp", "lead_rhino",  "amps/prs_archon/amp_arc50c_lead_rhino_std.nam"),
    ("amp", "lead_growl",  "amps/prs_archon/amp_arc50c_lead_growl_std.nam"),
    ("amp", "lead_nessie", "amps/prs_archon/amp_arc50c_lead_nessie_std.nam"),
    ("amp", "lead_kong",   "amps/prs_archon/amp_arc50c_lead_kong_std.nam"),
    ("pow", "p5_d5_cl",    "amps/prs_archon/pow_arc50c_p5_d5_cl_mesa4x12_std.nam"),
    ("pow", "p8_d2_rl",    "amps/prs_archon/pow_arc50c_p8_d2_rl_std.nam"),
    ("pow", "p6_d6_rl",    "amps/prs_archon/pow_arc50c_p6_d6_rl_std.nam"),
    ("pow", "p2_d6_rl",    "amps/prs_archon/pow_arc50c_p2_d6_rl_std.nam"),
    ("pow", "p5_d2_rl",    "amps/prs_archon/pow_arc50c_p5_d2_rl_std.nam"),
    ("pow", "p2_d8_rl",    "amps/prs_archon/pow_arc50c_p2_d8_rl_std.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "stage",
            "Stage",
            Some("Amp"),
            Some("amp"),
            &[
                ("amp", "Full Amp"),
                ("pow", "Power Amp"),
            ],
        ),
        enum_parameter(
            "tone",
            "Tone",
            Some("Amp"),
            Some("lead_hippo"),
            &[
                ("lead_hippo",  "Lead Hippo"),
                ("lead_roar",   "Lead Roar"),
                ("lead_rhino",  "Lead Rhino"),
                ("lead_growl",  "Lead Growl"),
                ("lead_nessie", "Lead Nessie"),
                ("lead_kong",   "Lead Kong"),
                ("p5_d5_cl",    "P5 D5 (Clean)"),
                ("p8_d2_rl",    "P8 D2 (Rhythm)"),
                ("p6_d6_rl",    "P6 D6 (Rhythm)"),
                ("p2_d6_rl",    "P2 D6 (Rhythm)"),
                ("p5_d2_rl",    "P5 D2 (Rhythm)"),
                ("p2_d8_rl",    "P2 D8 (Rhythm)"),
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
    let stage = required_string(params, "stage").map_err(anyhow::Error::msg)?;
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(s, t, _)| *s == stage && *t == tone)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for stage={} tone={}",
                MODEL_ID, stage, tone
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
