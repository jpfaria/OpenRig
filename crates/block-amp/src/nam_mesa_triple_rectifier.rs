use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_triple_rectifier";
pub const DISPLAY_NAME: &str = "Triple Rectifier";
const BRAND: &str = "mesa";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voicing × mic.
// All captures are TREC-150BLD-DIO. Voicings: Modern 6L6 Juice, Vintage 6L6 Juice,
// Vintage Chaosball. Mic: DI / SM57 / SM58 / Blend variants.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, mic, file)
    ("mdn_juice",  "di",     "amps/mesa_triple_rectifier/pow_trec_150bld_dio_mdn_6l6_juice_01_di.nam"),
    ("mdn_juice",  "sm57",   "amps/mesa_triple_rectifier/pow_trec_150bld_dio_mdn_6l6_juice_01_sm57.nam"),
    ("mdn_juice",  "sm58",   "amps/mesa_triple_rectifier/pow_trec_150bld_dio_mdn_6l6_juice_01_sm58.nam"),
    ("mdn_juice",  "blend2", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_mdn_6l6_juice_01_blend_2.nam"),
    ("mdn_juice",  "blend3", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_mdn_6l6_juice_01_blend_3.nam"),
    ("vnt_juice",  "di",     "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_di.nam"),
    ("vnt_juice",  "sm57",   "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_sm57.nam"),
    ("vnt_juice",  "sm58",   "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_sm58.nam"),
    ("vnt_juice",  "blend1", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_blend_1.nam"),
    ("vnt_juice",  "blend2", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_blend_2.nam"),
    ("vnt_juice",  "blend3", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_blend_3.nam"),
    ("chaosball",  "di",     "amps/mesa_triple_rectifier/amp_trec_150bld_dio_vnt_chaosball_di.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("mdn_juice"),
            &[
                ("mdn_juice", "Modern 6L6 Juice"),
                ("vnt_juice", "Vintage 6L6 Juice"),
                ("chaosball", "Vintage Chaosball"),
            ],
        ),
        enum_parameter(
            "mic",
            "Mic",
            Some("Amp"),
            Some("sm57"),
            &[
                ("di",     "DI (No Cab)"),
                ("sm57",   "SM57"),
                ("sm58",   "SM58"),
                ("blend1", "Blend #1"),
                ("blend2", "Blend #2"),
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
