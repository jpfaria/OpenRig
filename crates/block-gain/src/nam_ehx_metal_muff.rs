use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_ehx_metal_muff";
pub const DISPLAY_NAME: &str = "EHX Metal Muff";
const BRAND: &str = "ehx";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "as_an_overdrive",          model_path: "pedals/ehx_metal_muff/metal_muff_as_an_overdrive.nam" },
    NamCapture { tone: "eq_noon",                  model_path: "pedals/ehx_metal_muff/metal_muff_eq_noon.nam" },
    NamCapture { tone: "eq_noon_full_gain",        model_path: "pedals/ehx_metal_muff/metal_muff_eq_noon_full_gain.nam" },
    NamCapture { tone: "evil",                     model_path: "pedals/ehx_metal_muff/metal_muff_evil.nam" },
    NamCapture { tone: "mega_scoop",               model_path: "pedals/ehx_metal_muff/metal_muff_mega_scoop.nam" },
    NamCapture { tone: "my_settings",              model_path: "pedals/ehx_metal_muff/metal_muff_my_settings.nam" },
    NamCapture { tone: "nattens_madrigal",         model_path: "pedals/ehx_metal_muff/metal_muff_nattens_madrigal.nam" },
    NamCapture { tone: "swedish_chainsaw_ish",     model_path: "pedals/ehx_metal_muff/metal_muff_swedish_chainsaw_ish.nam" },
    NamCapture { tone: "tob_boost_low_distortion", model_path: "pedals/ehx_metal_muff/metal_muff_tob_boost_low_distortion.nam" },
    NamCapture { tone: "trve_kult",                model_path: "pedals/ehx_metal_muff/metal_muff_trve_kult.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("as_an_overdrive"),
        &[
            ("as_an_overdrive",          "As An Overdrive"),
            ("eq_noon",                  "Eq Noon"),
            ("eq_noon_full_gain",        "Eq Noon Full Gain"),
            ("evil",                     "Evil"),
            ("mega_scoop",               "Mega Scoop"),
            ("my_settings",              "My Settings"),
            ("nattens_madrigal",         "Nattens Madrigal"),
            ("swedish_chainsaw_ish",     "Swedish Chainsaw Ish"),
            ("tob_boost_low_distortion", "Tob Boost Low Distortion"),
            ("trve_kult",                "Trve Kult"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static NamCapture> {
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.tone == tone)
        .ok_or_else(|| anyhow!("gain model '{}' does not support tone='{}'", MODEL_ID, tone))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
