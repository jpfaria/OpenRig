use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_boogie_mark_vii";
pub const DISPLAY_NAME: &str = "Mark VII";
const BRAND: &str = "mesa";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Single-axis: 6 voicings spanning preamp-only / amp+cab / power section with
// SM58, SM57 or blend mics (sparse — pre/amp branches don't extend to mic axis).
const CAPTURES: &[(&str, &str, &str)] = &[
    ("pre_ch3",       "Pre Ch3 (MkIIC)",   "amps/mesa_boogie_mark_vii/pre_mesa_mkvii_90w_ch3_mkiic_eet_faq.nam"),
    ("amp_ch1_clean", "Amp Ch1 Clean",     "amps/mesa_boogie_mark_vii/amp_mesa_mkvii_90w_ch1_cln_factory_bright_clean_blend_2.nam"),
    ("pow_sm57",      "Power + SM57",      "amps/mesa_boogie_mark_vii/pow_mesa_mkvii_90w_eq_on_pres_5_sm57.nam"),
    ("pow_sm58",      "Power + SM58",      "amps/mesa_boogie_mark_vii/pow_mesa_mkvii_90w_eq_on_pres_5_sm58.nam"),
    ("pow_blend_2",   "Power + Blend 2",   "amps/mesa_boogie_mark_vii/pow_mesa_mkvii_90w_eq_on_pres_5_blend_2.nam"),
    ("pow_blend_3",   "Power + Blend 3",   "amps/mesa_boogie_mark_vii/pow_mesa_mkvii_90w_eq_on_pres_5_blend_3.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("pre_ch3"),
        &[
            ("pre_ch3",       "Pre Ch3 (MkIIC)"),
            ("amp_ch1_clean", "Amp Ch1 Clean"),
            ("pow_sm57",      "Power + SM57"),
            ("pow_sm58",      "Power + SM58"),
            ("pow_blend_2",   "Power + Blend 2"),
            ("pow_blend_3",   "Power + Blend 3"),
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
    let key = required_string(params, "preset").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no preset '{}'", MODEL_ID, key))
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
