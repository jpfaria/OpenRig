use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_dual_rectifier_rev_f";
pub const DISPLAY_NAME: &str = "Dual Rectifier Rev F";
const BRAND: &str = "mesa";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("black_album", "Black Album", "amps/mesa_dual_rectifier_rev_f/black_album.nam"),
    ("satan", "SATAN", "amps/mesa_dual_rectifier_rev_f/satan.nam"),
    ("chainsaw", "CHAINSAW", "amps/mesa_dual_rectifier_rev_f/chainsaw.nam"),
    ("hellbert", "HELLBERT", "amps/mesa_dual_rectifier_rev_f/hellbert.nam"),
    ("stealth", "STEALTH", "amps/mesa_dual_rectifier_rev_f/stealth.nam"),
    ("slaughter", "SLAUGHTER", "amps/mesa_dual_rectifier_rev_f/slaughter.nam"),
    ("far_beyond_driven", "Far beyond Driven", "amps/mesa_dual_rectifier_rev_f/far_beyond_driven.nam"),
    ("raw_gxx", "Raw GXX", "amps/mesa_dual_rectifier_rev_f/raw_gxx.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("black_album"),
        &[
            ("black_album",       "Black Album"),
            ("satan",             "Satan"),
            ("chainsaw",          "Chainsaw"),
            ("hellbert",          "Hellbert"),
            ("stealth",           "Stealth"),
            ("slaughter",         "Slaughter"),
            ("far_beyond_driven", "Far Beyond Driven"),
            ("raw_gxx",           "Raw GXX"),
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
