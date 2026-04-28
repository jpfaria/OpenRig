use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_matchless_clubman_35_head";
pub const DISPLAY_NAME: &str = "Matchless Clubman 35 head";
const BRAND: &str = "matchless";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voicing × input.
// SSR = Standard Switch position; Bright = bright channel.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, input, file)
    ("ssr_lead_2",   "hi", "amps/matchless_clubman_35_head/clubman_ssr_lead_2_hi_v03_b10_t12_br10_ma12.nam"),
    ("ssr_lead",     "lo", "amps/matchless_clubman_35_head/clubman_ssr_lead_lo_v04_b09_t11_br10_ma12.nam"),
    ("ssr_push",     "lo", "amps/matchless_clubman_35_head/clubman_ssr_push_lo_v12_b10_5_t12_br11_ma12.nam"),
    ("ssr_eob",      "lo", "amps/matchless_clubman_35_head/clubman_ssr_eob_lo_v10_b10_5_t12_br11_ma12.nam"),
    ("bright_push",  "lo", "amps/matchless_clubman_35_head/clubman_bright_push_lo_v12_b10_t12_br01_m12.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("ssr_lead"),
            &[
                ("ssr_lead",    "SSR Lead"),
                ("ssr_lead_2",  "SSR Lead 2"),
                ("ssr_push",    "SSR Push"),
                ("ssr_eob",     "SSR EoB"),
                ("bright_push", "Bright Push"),
            ],
        ),
        enum_parameter(
            "input",
            "Input",
            Some("Amp"),
            Some("lo"),
            &[
                ("lo", "Lo"),
                ("hi", "Hi"),
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
    let input = required_string(params, "input").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, i, _)| *v == voicing && *i == input)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} input={}",
                MODEL_ID, voicing, input
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
