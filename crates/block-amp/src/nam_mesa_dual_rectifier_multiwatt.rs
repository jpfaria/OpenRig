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

const CAPTURES: &[(&str, &str, &str)] = &[
    ("fr_p_2_t_5_m_4_b_2_v30_sm57c", "FR MBDR MW Red Mdn G-7 Ma-2 P-2 T-5 M-4 B-2 V30 SM57c", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_2_t_5_m_4_b_2_v30_sm57c.nam"),
    ("di_p_2_t_5_m_4_b_2", "DI MBDR MW Red Mdn G-7 Ma-2 P-2 T-5 M-4 B-2", "amps/mesa_dual_rectifier_multiwatt/di_mbdr_mw_red_mdn_g_7_ma_2_p_2_t_5_m_4_b_2.nam"),
    ("fr_p_2_t_5_m_4_b_2_m65_sm57b", "FR MBDR MW Red Mdn G-7 Ma-2 P-2 T-5 M-4 B-2 M65 SM57b", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_2_t_5_m_4_b_2_m65_sm57b.nam"),
    ("fr_p_3_t_6_m_4_b_3_m65_sm57b", "FR MBDR MW Red Mdn G-7 Ma-2 P-3 T-6 M-4 B-3 M65 SM57b", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_3_t_6_m_4_b_3_m65_sm57b.nam"),
    ("fr_p_4_t_4_m_3_b_4_m65_sm57a", "FR MBDR MW Red Mdn G-7 Ma-2 P-4 T-4 M-3 B-4 M65 SM57a", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_4_t_4_m_3_b_4_m65_sm57a.nam"),
    ("fr_p_4_t_4_m_3_b_4_v30_sm57b", "FR MBDR MW Red Mdn G-7 Ma-2 P-4 T-4 M-3 B-4 V30 SM57b", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_4_t_4_m_3_b_4_v30_sm57b.nam"),
    ("fr_p_2_t_5_m_4_b_2_v30_sm57b", "FR MBDR MW Red Mdn G-7 Ma-2 P-2 T-5 M-4 B-2 V30 SM57b", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_2_t_5_m_4_b_2_v30_sm57b.nam"),
    ("fr_p_4_t_4_m_3_b_4_m65_sm57b", "FR MBDR MW Red Mdn G-7 Ma-2 P-4 T-4 M-3 B-4 M65 SM57b", "amps/mesa_dual_rectifier_multiwatt/fr_mbdr_mw_red_mdn_g_7_ma_2_p_4_t_4_m_3_b_4_m65_sm57b.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("fr_p_2_t_5_m_4_b_2_v30_sm57c"),
        &[
            ("fr_p_2_t_5_m_4_b_2_v30_sm57c", "FR MBDR MW Red Mdn G-7 Ma-2 P-2 T-5 M-4 B-2 V30 SM57c"),
            ("di_p_2_t_5_m_4_b_2", "DI MBDR MW Red Mdn G-7 Ma-2 P-2 T-5 M-4 B-2"),
            ("fr_p_2_t_5_m_4_b_2_m65_sm57b", "FR MBDR MW Red Mdn G-7 Ma-2 P-2 T-5 M-4 B-2 M65 SM57b"),
            ("fr_p_3_t_6_m_4_b_3_m65_sm57b", "FR MBDR MW Red Mdn G-7 Ma-2 P-3 T-6 M-4 B-3 M65 SM57b"),
            ("fr_p_4_t_4_m_3_b_4_m65_sm57a", "FR MBDR MW Red Mdn G-7 Ma-2 P-4 T-4 M-3 B-4 M65 SM57a"),
            ("fr_p_4_t_4_m_3_b_4_v30_sm57b", "FR MBDR MW Red Mdn G-7 Ma-2 P-4 T-4 M-3 B-4 V30 SM57b"),
            ("fr_p_2_t_5_m_4_b_2_v30_sm57b", "FR MBDR MW Red Mdn G-7 Ma-2 P-2 T-5 M-4 B-2 V30 SM57b"),
            ("fr_p_4_t_4_m_3_b_4_m65_sm57b", "FR MBDR MW Red Mdn G-7 Ma-2 P-4 T-4 M-3 B-4 M65 SM57b"),
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
