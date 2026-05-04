use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_dual_rectifier_multiwatt";
pub const DISPLAY_NAME: &str = "Dual Rectifier Multi-Watt";
const BRAND: &str = "mesa";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: EQ setting × cab+mic.
// All captures are Red channel, Modern mode, G7 / Master 2.
// EQ encodes Presence/Treble/Mid/Bass: balanced (P2/T5/M4/B2),
// mid_cut (P3/T6/M4/B3), or scoop (P4/T4/M3/B4).
const CAPTURES: &[(&str, &str, &str)] = &[
    // (eq, cab_mic, file)
    ("balanced", "di",        "amps/mesa_dual_rectifier_multiwatt/di_mbdr_mw_red_mdn_g_7_ma_2_p_2_t_5_m_4_b_2.nam"),
    ("balanced", "v30_sm57b", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_2_t_5_m_4_b_2_v30_sm57b.nam"),
    ("balanced", "v30_sm57c", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_2_t_5_m_4_b_2_v30_sm57c.nam"),
    ("balanced", "m65_sm57b", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_2_t_5_m_4_b_2_m65_sm57b.nam"),
    ("mid_cut",  "m65_sm57b", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_3_t_6_m_4_b_3_m65_sm57b.nam"),
    ("scoop",    "v30_sm57b", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_4_t_4_m_3_b_4_v30_sm57b.nam"),
    ("scoop",    "m65_sm57a", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_4_t_4_m_3_b_4_m65_sm57a.nam"),
    ("scoop",    "m65_sm57b", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_4_t_4_m_3_b_4_m65_sm57b.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "eq",
            "EQ",
            Some("Amp"),
            Some("balanced"),
            &[
                ("balanced", "Balanced"),
                ("mid_cut",  "Mid Cut"),
                ("scoop",    "Scoop"),
            ],
        ),
        enum_parameter(
            "cab_mic",
            "Cab + Mic",
            Some("Amp"),
            Some("v30_sm57b"),
            &[
                ("di",        "DI (No Cab)"),
                ("v30_sm57b", "V30 + SM57 (B)"),
                ("v30_sm57c", "V30 + SM57 (C)"),
                ("m65_sm57a", "M65 + SM57 (A)"),
                ("m65_sm57b", "M65 + SM57 (B)"),
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
    let eq = required_string(params, "eq").map_err(anyhow::Error::msg)?;
    let cab_mic = required_string(params, "cab_mic").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(e, c, _)| *e == eq && *c == cab_mic)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for eq={} cab_mic={}",
                MODEL_ID, eq, cab_mic
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
