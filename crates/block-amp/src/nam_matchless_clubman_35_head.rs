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

const CAPTURES: &[(&str, &str, &str)] = &[
    ("clubman_ssr_lead_2_hi_v03_b10_t1", "Clubman SSR Lead 2 Hi v03 b10  t12 br10 ma12", "amps/matchless_clubman_35_head/clubman_ssr_lead_2_hi_v03_b10_t12_br10_ma12.nam"),
    ("clubman_ssr_lead_lo_v04_b09_t11_", "Clubman SSR Lead Lo v04 b09  t11 br10 ma12", "amps/matchless_clubman_35_head/clubman_ssr_lead_lo_v04_b09_t11_br10_ma12.nam"),
    ("clubman_ssr_push_lo_v12_b10_5_t1", "Clubman SSR Push Lo v12 b10.5 t12 br11 ma12", "amps/matchless_clubman_35_head/clubman_ssr_push_lo_v12_b10_5_t12_br11_ma12.nam"),
    ("clubman_ssr_eob_lo_v10_b10_5_t12", "Clubman SSR EoB Lo v10 b10.5 t12 br11 ma12", "amps/matchless_clubman_35_head/clubman_ssr_eob_lo_v10_b10_5_t12_br11_ma12.nam"),
    ("clubman_bright_push_lo_v12_b10_t", "Clubman bright Push Lo v12 b10 t12 br01 m12", "amps/matchless_clubman_35_head/clubman_bright_push_lo_v12_b10_t12_br01_m12.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("clubman_ssr_lead_2_hi_v03_b10_t1"),
        &[
            ("clubman_ssr_lead_2_hi_v03_b10_t1", "Clubman SSR Lead 2 Hi v03 b10  t12 br10 ma12"),
            ("clubman_ssr_lead_lo_v04_b09_t11_", "Clubman SSR Lead Lo v04 b09  t11 br10 ma12"),
            ("clubman_ssr_push_lo_v12_b10_5_t1", "Clubman SSR Push Lo v12 b10.5 t12 br11 ma12"),
            ("clubman_ssr_eob_lo_v10_b10_5_t12", "Clubman SSR EoB Lo v10 b10.5 t12 br11 ma12"),
            ("clubman_bright_push_lo_v12_b10_t", "Clubman bright Push Lo v12 b10 t12 br01 m12"),
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
